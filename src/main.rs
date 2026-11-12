mod animation;
mod bitmap;
mod cli;
mod config;
mod cpu;
mod desktop;
mod gpu;
mod logo;
mod memory;
mod network;
mod os;
mod output;
mod storage;
mod system;
mod theme;
mod tui;

use clap::Parser;
use cli::Cli;
use config::{Frame, LayoutStyle, Mode, ModuleEntry};
use logo::Art;
use output::{Charset, ComposeOpts, Ctx};
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use system::{CollectOptions, ModuleKind};
use theme::ColorDepth;

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(e) if e.contains("Broken pipe") => 0,
        Err(e) => {
            eprintln!("openfetch: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn emit(text: &str) -> Result<(), String> {
    let mut out = io::stdout().lock();
    out.write_all(text.as_bytes())
        .and_then(|_| out.flush())
        .map_err(|e| e.to_string())
}

fn env_var(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// Terminal width in columns, if known.
fn terminal_width() -> Option<usize> {
    if io::stdout().is_terminal() {
        if let Ok((w, _)) = crossterm::terminal::size() {
            if w > 0 {
                return Some(w as usize);
            }
        }
    }
    env_var("COLUMNS")
        .and_then(|c| c.parse().ok())
        .filter(|w| *w > 0)
}

/// Terminal height in rows, if known.
fn terminal_height() -> Option<usize> {
    if io::stdout().is_terminal() {
        if let Ok((_, h)) = crossterm::terminal::size() {
            if h > 0 {
                return Some(h as usize);
            }
        }
    }
    env_var("LINES")
        .and_then(|c| c.parse().ok())
        .filter(|h| *h > 0)
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    if cli.list_themes {
        return emit(&format!("{}\n", theme::BUILTIN_NAMES.join("\n")));
    }
    if cli.list_logos {
        let list: String = logo::BUILTINS
            .iter()
            .map(|b| format!("{:<10} {}\n", b.id, b.display))
            .collect();
        return emit(&list);
    }
    if cli.init_config {
        let path = cli
            .config_path()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(config::default_path);
        config::init(&path)?;
        return emit(&format!("Wrote {}\n", path.display()));
    }

    let loaded = config::load(cli.config_path())?;
    let cfg = loaded.config;

    if cli.show_config() {
        let dump = toml::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        let status = if loaded.found {
            "found"
        } else {
            "not found, using built-in defaults; create it with `openfetch --init-config`"
        };
        return emit(&format!(
            "# Config file: {} ({status})\n# Effective configuration:\n\n{dump}",
            loaded.path.display()
        ));
    }

    // ---- presentation settings
    let stdout_tty = io::stdout().is_terminal();
    let color_on = !cli.no_color
        && match cfg.general.color {
            Mode::Always => true,
            Mode::Never => false,
            Mode::Auto => stdout_tty && env_var("NO_COLOR").map_or(true, |v| v.is_empty()),
        };
    let depth = if color_on {
        ColorDepth::detect()
    } else {
        ColorDepth::None
    };
    let unicode = !cli.ascii
        && match cfg.general.unicode {
            Mode::Always => true,
            Mode::Never => false,
            Mode::Auto => output::detect_unicode(env_var),
        };
    let cs = Charset { unicode };

    let theme_name = cli
        .theme
        .clone()
        .unwrap_or_else(|| cfg.general.theme.clone());
    let theme = theme::resolve(&theme_name, &cfg.themes).unwrap_or_else(|e| {
        eprintln!("openfetch: {e}; using the default theme");
        theme::Theme::default()
    });

    let compact = cli.compact || cfg.layout.style == LayoutStyle::Compact;

    // Privacy: with --noip nothing network-related is even collected.
    let hide_ip = cli.noip || cfg.general.hide_ip;

    // ---- JSON output
    if cli.json {
        let mut wanted: HashSet<ModuleKind> = ModuleKind::ALL.into_iter().collect();
        if hide_ip {
            wanted.remove(&ModuleKind::Network);
            wanted.remove(&ModuleKind::LocalIp);
        }
        let opts = CollectOptions {
            sample_cpu: true,
            hide_mounts: cfg.general.hide_mounts.clone(),
        };
        let info = system::collect(&wanted, &opts);
        let json = serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?;
        return emit(&format!("{json}\n"));
    }

    // ---- logo (text art or a picture)
    let os_release = os::detect_os();
    let override_name = cli.distro.as_deref().or(cli.logo_only().flatten());
    let selected = logo::select(&cfg.logo, override_name, Some(&os_release))?;
    let palette = cfg
        .logo
        .colors
        .clone()
        .or_else(|| theme.logo.clone())
        .unwrap_or_else(|| selected.palette.clone());
    let max_rows = terminal_height()
        .unwrap_or(34)
        .saturating_sub(4)
        .clamp(6, 40);
    let image_path = cli.image.clone().or_else(|| cfg.logo.image.clone());
    let text_art = || selected.art(compact, unicode, &cfg.logo.style, &palette);
    let art = match image_path {
        Some(p) => {
            let path = config::expand_tilde(&p);
            let cols = cfg.logo.image_width.clamp(8, 120) / if compact { 2 } else { 1 };
            match bitmap::load_image(&path, cols, max_rows) {
                Ok(frames) => Art::Images(frames),
                Err(e) if cli.image.is_some() => return Err(e),
                Err(e) => {
                    eprintln!("openfetch: {e}; using the normal logo");
                    text_art()
                }
            }
        }
        None => text_art(),
    };

    // without colours a picture is drawn as one solid shape
    let art = if depth == ColorDepth::None && !cli.tui {
        art.mono()
    } else {
        art
    };

    if cli.logo_only().is_some() {
        return emit(&output::render_ansi(&art.lines(&palette), depth));
    }

    // ---- information
    let panels = !compact && cfg.layout.frame == Frame::Panels;
    let entries: Vec<ModuleEntry> = if panels || cli.tui {
        cfg.sections
            .iter()
            .flat_map(|s| s.modules.clone())
            .collect()
    } else {
        cfg.active_modules(compact)
    };
    let mut wanted: HashSet<ModuleKind> = HashSet::new();
    for e in &entries {
        if let ModuleEntry::Name(n) = e {
            match ModuleKind::from_name(n) {
                Some(k) => {
                    wanted.insert(k);
                }
                None => eprintln!("openfetch: unknown module '{n}' in config (ignored)"),
            }
        }
    }

    if hide_ip {
        wanted.remove(&ModuleKind::Network);
        wanted.remove(&ModuleKind::LocalIp);
    }

    // ---- live dashboard
    if cli.tui {
        if !stdout_tty || !io::stdin().is_terminal() {
            return Err("--tui needs an interactive terminal".into());
        }
        let opts = CollectOptions {
            sample_cpu: false,
            hide_mounts: cfg.general.hide_mounts.clone(),
        };
        let info = system::collect(&wanted, &opts);
        // an animated GIF always plays its own frames
        let is_gif = matches!(&art, Art::Images(f) if f.len() > 1);
        let anim = match cfg.tui.effect.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "" => None,
            name => match animation::build(
                if is_gif { "auto" } else { name },
                &art,
                &palette,
                &cfg.animation,
            ) {
                Ok(a) => Some(a),
                Err(e) => {
                    eprintln!("openfetch: {e}; the logo will not animate");
                    None
                }
            },
        };
        let interval = cli
            .interval
            .unwrap_or(cfg.tui.interval_ms)
            .clamp(250, 10_000);
        let tui_opts = tui::Options {
            interval: std::time::Duration::from_millis(interval),
            fps: cfg.animation.fps,
        };
        let still = art.lines(&palette);
        let app = tui::App::new(cfg.clone(), info, theme_name, theme, cs, depth, still, anim);
        return tui::run(app, tui_opts).map_err(|e| e.to_string());
    }

    let opts = CollectOptions {
        sample_cpu: cfg.general.cpu_usage,
        hide_mounts: cfg.general.hide_mounts.clone(),
    };
    let info = system::collect(&wanted, &opts);
    let ctx = Ctx {
        info: &info,
        cfg: &cfg,
        theme: &theme,
        cs,
        compact,
        show_labels: if panels { cfg.layout.show_labels } else { true },
    };

    let compose_opts = ComposeOpts {
        position: cfg.layout.logo_position,
        gap: cfg.layout.gap.min(20),
        margin: cfg.layout.margin.min(20),
        max_width: terminal_width(),
        ellipsis: cs.ellipsis(),
    };
    let info_lines = if panels {
        let budget = output::info_budget(art.width(), &compose_opts);
        output::build_panels(&ctx, &cfg.sections, budget)
    } else {
        output::build_info(&ctx, &entries)
    };

    // ---- animation (never when piped)
    let requested = cli.animation_request();
    let animate = (requested.is_some() || cfg.animation.enabled) && stdout_tty;
    if animate {
        let name = requested
            .flatten()
            .map(str::to_string)
            .unwrap_or_else(|| cfg.animation.effect.clone());
        let built = animation::build(&name, &art, &palette, &cfg.animation).or_else(|e| {
            eprintln!("openfetch: {e}; using rainbow");
            animation::build("rainbow", &art, &palette, &cfg.animation)
        });
        match built {
            Ok(anim) => {
                let compose = |logo_lines: Vec<output::Line>| {
                    output::compose(&logo_lines, &info_lines, &compose_opts)
                };
                match animation::run(&anim, cfg.animation.duration, depth, &compose) {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        eprintln!("openfetch: animation unavailable ({e}); showing static output")
                    }
                }
            }
            Err(e) => eprintln!("openfetch: {e}"),
        }
    }

    let lines = output::compose(&art.lines(&palette), &info_lines, &compose_opts);
    emit(&output::render_ansi(&lines, depth))
}
