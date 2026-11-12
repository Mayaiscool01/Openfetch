//! Live full-screen dashboard: `openfetch --tui`.
//!
//! The same boxed sections as the normal output, plus live CPU / memory /
//! disk / network panels, with the logo (optionally spinning) beside them.

use crate::animation::{ratatui_color, to_ratatui, Animation};
use crate::config::{Config, LogoPosition, ModuleEntry, SectionConfig};
use crate::memory::percent;
use crate::output::{self, format_bytes, format_uptime, line_width, Charset, Ctx, Line};
use crate::storage::{self, DiskInfo};
use crate::system::{ModuleKind, SystemInfo};
use crate::theme::{self, ColorDepth, ColorSpec, Theme};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line as RLine, Span as RSpan};
use ratatui::widgets::{Block, Borders, Gauge, Padding, Paragraph, Sparkline};
use ratatui::{Frame, Terminal};
use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

/// Modules that the live panels replace.
const LIVE_MODULES: [ModuleKind; 9] = [
    ModuleKind::CpuUsage,
    ModuleKind::CpuTemp,
    ModuleKind::Memory,
    ModuleKind::Swap,
    ModuleKind::Disk,
    ModuleKind::Network,
    ModuleKind::Load,
    ModuleKind::Uptime,
    ModuleKind::Processes,
];

/// Sections with the live modules taken out (empty sections disappear).
pub fn static_sections(sections: &[SectionConfig]) -> Vec<SectionConfig> {
    sections
        .iter()
        .map(|s| SectionConfig {
            modules: s
                .modules
                .iter()
                .filter(|m| match m {
                    ModuleEntry::Name(n) => {
                        ModuleKind::from_name(n).map_or(true, |k| !LIVE_MODULES.contains(&k))
                    }
                    ModuleEntry::Custom { .. } => true,
                })
                .cloned()
                .collect(),
            ..s.clone()
        })
        .collect()
}

// ------------------------------------------------------------------ data

/// Everything that changes while the dashboard runs.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub cpu_total: f32,
    pub cores: Vec<f32>,
    pub cpu_temp: Option<f32>,
    pub cpu_history: VecDeque<u64>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub disks: Vec<DiskInfo>,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub rx_history: VecDeque<u64>,
    pub tx_history: VecDeque<u64>,
    pub load: Option<[f32; 3]>,
    pub processes: Option<usize>,
    pub uptime: Option<u64>,
}

pub fn push_history(h: &mut VecDeque<u64>, value: u64, cap: usize) {
    h.push_back(value);
    while h.len() > cap.max(1) {
        h.pop_front();
    }
}

pub fn format_rate(bytes_per_sec: f64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let mut v = bytes_per_sec.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", UNITS[0])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Reads the system on every tick.
struct Sampler {
    sys: sysinfo::System,
    networks: sysinfo::Networks,
    last: Instant,
    ticks: u64,
    hide_mounts: Vec<String>,
}

impl Sampler {
    fn new(hide_mounts: Vec<String>) -> Sampler {
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_all();
        Sampler {
            sys,
            networks: sysinfo::Networks::new_with_refreshed_list(),
            last: Instant::now(),
            ticks: 0,
            hide_mounts,
        }
    }

    fn sample(&mut self, live: &mut Live, history: usize) {
        let dt = self.last.elapsed().as_secs_f64().max(0.05);
        self.last = Instant::now();

        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        live.cpu_total = self.sys.global_cpu_usage();
        live.cores = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        live.cpu_temp = crate::cpu::temperature();
        push_history(
            &mut live.cpu_history,
            live.cpu_total.round() as u64,
            history,
        );
        live.mem_used = self.sys.used_memory();
        live.mem_total = self.sys.total_memory();
        live.swap_used = self.sys.used_swap();
        live.swap_total = self.sys.total_swap();

        self.networks.refresh();
        let (mut rx, mut tx) = (0u64, 0u64);
        for (name, data) in &self.networks {
            if !crate::network::is_virtual(name) {
                rx += data.received();
                tx += data.transmitted();
            }
        }
        live.rx_rate = rx as f64 / dt;
        live.tx_rate = tx as f64 / dt;
        push_history(&mut live.rx_history, live.rx_rate as u64, history);
        push_history(&mut live.tx_history, live.tx_rate as u64, history);

        if self.ticks % 5 == 0 {
            live.disks = storage::detect(&self.hide_mounts);
        }
        live.load = crate::os::load_average();
        live.processes = crate::os::process_count();
        live.uptime = crate::os::uptime_seconds();
        self.ticks += 1;
    }
}

// ------------------------------------------------------------------- app

pub struct App {
    cfg: Config,
    info: SystemInfo,
    theme: Theme,
    theme_name: String,
    cs: Charset,
    depth: ColorDepth,
    still: Vec<Line>,
    anim: Option<Animation>,
    pub spinning: bool,
    pub paused: bool,
    started: Instant,
    sections: Vec<(String, Vec<Line>)>,
    pub live: Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Info(usize),
    Cpu,
    Mem,
    Disks,
    Net,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: Config,
        info: SystemInfo,
        theme_name: String,
        theme: Theme,
        cs: Charset,
        depth: ColorDepth,
        still: Vec<Line>,
        anim: Option<Animation>,
    ) -> App {
        let spinning = anim.is_some();
        let mut app = App {
            cfg,
            info,
            theme,
            theme_name,
            cs,
            depth,
            still,
            anim,
            spinning,
            paused: false,
            started: Instant::now(),
            sections: Vec::new(),
            live: Live::default(),
        };
        app.rebuild_sections();
        app
    }

    fn rebuild_sections(&mut self) {
        let ctx = Ctx {
            info: &self.info,
            cfg: &self.cfg,
            theme: &self.theme,
            cs: self.cs,
            compact: false,
            show_labels: self.cfg.layout.show_labels,
        };
        self.sections = static_sections(&self.cfg.sections)
            .iter()
            .filter_map(|sec| {
                let mut lines = output::section_lines(&ctx, sec);
                // removing live modules can leave doubled gaps
                lines.dedup_by(|a, b| a.is_empty() && b.is_empty());
                lines
                    .iter()
                    .any(|l| !l.is_empty())
                    .then(|| (sec.title.clone(), lines))
            })
            .collect();
    }

    /// Switch to the next theme (built-ins first, then custom ones).
    pub fn next_theme(&mut self) {
        let mut names: Vec<String> = theme::BUILTIN_NAMES.iter().map(|s| s.to_string()).collect();
        for k in self.cfg.themes.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
        let at = names
            .iter()
            .position(|n| *n == self.theme_name)
            .unwrap_or(0);
        for step in 1..=names.len() {
            let name = &names[(at + step) % names.len()];
            if let Ok(t) = theme::resolve(name, &self.cfg.themes) {
                self.theme = t;
                self.theme_name = name.clone();
                break;
            }
        }
        self.rebuild_sections();
    }

    #[cfg(test)]
    pub fn theme_name(&self) -> &str {
        &self.theme_name
    }

    fn animating(&self) -> bool {
        self.spinning && self.anim.is_some()
    }

    fn logo_lines(&self) -> Vec<Line> {
        match (&self.anim, self.spinning) {
            (Some(a), true) => a.frame(self.started.elapsed().as_secs_f32() * a.speed),
            _ => self.still.clone(),
        }
    }

    fn rc(&self, c: ColorSpec) -> ratatui::style::Color {
        ratatui_color(c, self.depth)
    }

    fn level(&self, pct: f32) -> ColorSpec {
        if pct >= 90.0 {
            self.theme.critical
        } else if pct >= 70.0 {
            self.theme.warn
        } else {
            self.theme.bar
        }
    }

    /// Which panels fit into `height` rows; the least important go first.
    fn plan(&self, height: u16) -> Vec<(Panel, u16)> {
        let mut panels: Vec<(Panel, u16)> = Vec::new();
        for (i, (_, lines)) in self.sections.iter().enumerate() {
            panels.push((Panel::Info(i), lines.len() as u16 + 2));
        }
        panels.push((Panel::Cpu, 6));
        if self.live.mem_total > 0 {
            panels.push((Panel::Mem, 2 + if self.live.swap_total > 0 { 2 } else { 1 }));
        }
        if !self.live.disks.is_empty() {
            panels.push((Panel::Disks, self.live.disks.len().min(6) as u16 + 2));
        }
        panels.push((Panel::Net, 5));

        let total = |p: &[(Panel, u16)]| p.iter().map(|(_, h)| *h).sum::<u16>();
        let drop_order = |p: &[(Panel, u16)]| -> Option<usize> {
            for kind in [Panel::Net, Panel::Disks] {
                if let Some(i) = p.iter().position(|(k, _)| *k == kind) {
                    return Some(i);
                }
            }
            if let Some(i) = p.iter().rposition(|(k, _)| matches!(k, Panel::Info(_))) {
                return Some(i);
            }
            for kind in [Panel::Mem, Panel::Cpu] {
                if let Some(i) = p.iter().position(|(k, _)| *k == kind) {
                    return Some(i);
                }
            }
            None
        };
        while total(&panels) > height {
            match drop_order(&panels) {
                Some(i) => {
                    panels.remove(i);
                }
                None => break,
            }
        }
        panels
    }

    fn block(&self, title: &str) -> Block<'static> {
        let b = Block::default()
            .borders(Borders::ALL)
            .padding(Padding::horizontal(1))
            .border_style(Style::default().fg(self.rc(self.theme.rule)))
            .title(RSpan::styled(
                format!(" {title} "),
                Style::default()
                    .fg(self.rc(self.theme.title))
                    .add_modifier(Modifier::BOLD),
            ));
        if self.cs.unicode {
            b.border_type(ratatui::widgets::BorderType::Rounded)
        } else {
            b.border_set(symbols::border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "-",
                horizontal_bottom: "-",
            })
        }
    }

    fn gauge(&self, pct: f32, label: String) -> Gauge<'static> {
        Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(self.rc(self.level(pct)))
                    .bg(self.rc(self.theme.bar_empty)),
            )
            .ratio((pct as f64 / 100.0).clamp(0.0, 1.0))
            .use_unicode(self.cs.unicode)
            .label(RSpan::styled(
                label,
                Style::default().add_modifier(Modifier::BOLD),
            ))
    }

    fn core_glyphs(&self, width: usize) -> RLine<'static> {
        let glyphs: &[char] = if self.cs.unicode {
            &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']
        } else {
            &['_', '.', ':', '-', '=', '+', '*', '#']
        };
        let mut spans = vec![RSpan::styled(
            "cores ",
            Style::default().fg(self.rc(self.theme.separator)),
        )];
        for u in self.live.cores.iter().take(width.saturating_sub(6)) {
            let idx = ((u.clamp(0.0, 100.0) / 100.0) * 7.0).round() as usize;
            spans.push(RSpan::styled(
                glyphs[idx.min(7)].to_string(),
                Style::default().fg(self.rc(self.level(*u))),
            ));
        }
        RLine::from(spans)
    }

    fn footer(&self) -> RLine<'static> {
        let dim = Style::default().fg(self.rc(self.theme.separator));
        let val = Style::default().fg(self.rc(self.theme.value));
        let keys = if self.cs.unicode {
            "q quit · t theme · s spin · p pause"
        } else {
            "q quit | t theme | s spin | p pause"
        };
        let mut spans = vec![RSpan::styled(keys, dim)];
        if self.paused {
            spans.push(RSpan::styled(
                "  [paused]",
                Style::default().fg(self.rc(self.theme.warn)),
            ));
        }
        let mut right = Vec::new();
        if let Some(u) = self.live.uptime {
            right.push(format!("up {}", format_uptime(u)));
        }
        if let Some([a, b, c]) = self.live.load {
            right.push(format!("load {a:.2} {b:.2} {c:.2}"));
        }
        if let Some(n) = self.live.processes {
            right.push(format!("{n} procs"));
        }
        if !right.is_empty() {
            spans.push(RSpan::styled("   ", dim));
            spans.push(RSpan::styled(right.join("  "), val));
        }
        RLine::from(spans)
    }
}

// ------------------------------------------------------------------ view

fn vsplit(area: Rect, rows: &[u16]) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            rows.iter()
                .map(|h| Constraint::Length(*h))
                .collect::<Vec<_>>(),
        )
        .split(area)
        .to_vec()
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.size();
    if area.height < 4 || area.width < 24 {
        f.render_widget(Paragraph::new("terminal too small"), area);
        return;
    }
    let body = Rect::new(area.x, area.y, area.width, area.height - 1);
    let footer = Rect::new(area.x, area.bottom() - 1, area.width, 1);

    let logo = app.logo_lines();
    let logo_w = logo.iter().map(line_width).max().unwrap_or(0) as u16;
    let show_logo = logo_w > 0
        && app.cfg.layout.logo_position != LogoPosition::None
        && logo_w + 3 + 40 <= body.width;
    let right = if show_logo {
        let logo_h = (logo.len() as u16).min(body.height);
        f.render_widget(
            Paragraph::new(to_ratatui(&logo, app.depth)),
            Rect::new(body.x + 1, body.y, logo_w, logo_h),
        );
        Rect::new(
            body.x + logo_w + 3,
            body.y,
            body.width - logo_w - 3,
            body.height,
        )
    } else {
        body
    };

    let plan = app.plan(right.height);
    let heights: Vec<u16> = plan.iter().map(|(_, h)| *h).collect();
    for ((panel, _), rect) in plan.iter().zip(vsplit(right, &heights)) {
        match panel {
            Panel::Info(i) => draw_info(f, app, *i, rect),
            Panel::Cpu => draw_cpu(f, app, rect),
            Panel::Mem => draw_mem(f, app, rect),
            Panel::Disks => draw_disks(f, app, rect),
            Panel::Net => draw_net(f, app, rect),
        }
    }
    f.render_widget(Paragraph::new(app.footer()), footer);
}

fn draw_info(f: &mut Frame, app: &App, i: usize, rect: Rect) {
    let (title, lines) = &app.sections[i];
    f.render_widget(
        Paragraph::new(to_ratatui(lines, app.depth)).block(app.block(title)),
        rect,
    );
}

fn draw_cpu(f: &mut Frame, app: &App, rect: Rect) {
    let block = app.block("CPU");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let l = &app.live;
    let mut label = format!("{:.0}%", l.cpu_total);
    if let Some(t) = l.cpu_temp {
        label.push_str(&format!("  {t:.0}{}", app.cs.degrees()));
    }
    let rows = vsplit(inner, &[1, 2, 1]);
    f.render_widget(app.gauge(l.cpu_total, label), rows[0]);
    let data: Vec<u64> = l.cpu_history.iter().copied().collect();
    f.render_widget(
        Sparkline::default()
            .data(&data)
            .max(100)
            .style(Style::default().fg(app.rc(app.theme.bar))),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(app.core_glyphs(inner.width as usize)),
        rows[2],
    );
}

fn usage_label(name: &str, used: u64, total: u64) -> String {
    format!(
        "{name} {} / {} ({:.0}%)",
        format_bytes(used),
        format_bytes(total),
        percent(used, total)
    )
}

fn draw_mem(f: &mut Frame, app: &App, rect: Rect) {
    let block = app.block("Memory");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let l = &app.live;
    let mut rows = vec![1u16];
    if l.swap_total > 0 {
        rows.push(1);
    }
    let areas = vsplit(inner, &rows);
    let p = percent(l.mem_used, l.mem_total);
    f.render_widget(
        app.gauge(p, usage_label("RAM", l.mem_used, l.mem_total)),
        areas[0],
    );
    if l.swap_total > 0 {
        let p = percent(l.swap_used, l.swap_total);
        f.render_widget(
            app.gauge(p, usage_label("swap", l.swap_used, l.swap_total)),
            areas[1],
        );
    }
}

fn draw_disks(f: &mut Frame, app: &App, rect: Rect) {
    let block = app.block("Disks");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let shown: Vec<&DiskInfo> = app.live.disks.iter().take(inner.height as usize).collect();
    let areas = vsplit(inner, &vec![1; shown.len()]);
    for (d, area) in shown.iter().zip(areas) {
        let p = percent(d.used, d.total);
        let label = format!(
            "{} {} / {} ({:.0}%) {}",
            d.mount_point,
            format_bytes(d.used),
            format_bytes(d.total),
            p,
            d.fs_type
        );
        f.render_widget(app.gauge(p, label), area);
    }
}

fn draw_net(f: &mut Frame, app: &App, rect: Rect) {
    let block = app.block("Network");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let l = &app.live;
    let (down, up) = if app.cs.unicode {
        ("↓", "↑")
    } else {
        ("v", "^")
    };
    let line = RLine::from(vec![
        RSpan::styled(
            format!("{down} {}", format_rate(l.rx_rate)),
            Style::default().fg(app.rc(app.theme.bar)),
        ),
        RSpan::raw("   "),
        RSpan::styled(
            format!("{up} {}", format_rate(l.tx_rate)),
            Style::default().fg(app.rc(app.theme.key)),
        ),
    ]);
    let rows = vsplit(inner, &[1, 1, 1]);
    f.render_widget(Paragraph::new(line), rows[0]);
    let rx: Vec<u64> = l.rx_history.iter().copied().collect();
    let tx: Vec<u64> = l.tx_history.iter().copied().collect();
    f.render_widget(
        Sparkline::default()
            .data(&rx)
            .style(Style::default().fg(app.rc(app.theme.bar))),
        rows[1],
    );
    f.render_widget(
        Sparkline::default()
            .data(&tx)
            .style(Style::default().fg(app.rc(app.theme.key))),
        rows[2],
    );
}

// ------------------------------------------------------------------- run

pub struct Options {
    pub interval: Duration,
    pub fps: u32,
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Run until the user quits. Restores the terminal on exit and on panic.
pub fn run(mut app: App, opts: Options) -> io::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;

    let history = app.cfg.tui.history.clamp(10, 600);
    let mut sampler = Sampler::new(app.cfg.general.hide_mounts.clone());
    // sysinfo needs two readings a moment apart to compute CPU usage
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL + Duration::from_millis(20));
    sampler.sample(&mut app.live, history);
    let mut next_sample = Instant::now() + opts.interval;
    let frame = Duration::from_millis(1000 / opts.fps.clamp(1, 60) as u64);

    loop {
        terminal.draw(|f| draw(f, &app))?;

        let until_sample = next_sample.saturating_duration_since(Instant::now());
        let wait = if app.animating() {
            frame.min(until_sample)
        } else {
            until_sample
        };
        if event::poll(wait)? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Char('t') => app.next_theme(),
                        KeyCode::Char('s') => app.spinning = !app.spinning && app.anim.is_some(),
                        KeyCode::Char('p') => app.paused = !app.paused,
                        _ => {}
                    }
                }
            }
        }
        if Instant::now() >= next_sample {
            if !app.paused {
                sampler.sample(&mut app.live, history);
            }
            next_sample = Instant::now() + opts.interval;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::CpuInfo;
    use crate::os::OsRelease;
    use crate::output::Span;
    use ratatui::backend::TestBackend;

    fn info() -> SystemInfo {
        SystemInfo {
            username: Some("mia".into()),
            hostname: Some("asahi".into()),
            os: Some(OsRelease {
                pretty_name: "Fedora Linux 41".into(),
                arch: "aarch64".into(),
                ..Default::default()
            }),
            kernel: Some("6.11.0".into()),
            cpu: Some(CpuInfo {
                model: "Apple M1".into(),
                cores: Some(8),
                threads: 8,
                max_mhz: Some(3200),
                usage_percent: None,
                temperature_c: None,
            }),
            ..Default::default()
        }
    }

    fn app(unicode: bool) -> App {
        let mut a = App::new(
            Config::default(),
            info(),
            "default".into(),
            Theme::default(),
            Charset { unicode },
            ColorDepth::TrueColor,
            vec![vec![Span::plain("LOGO".to_string())]; 6],
            None,
        );
        a.live = Live {
            cpu_total: 42.0,
            cores: vec![5.0, 50.0, 95.0, 20.0],
            cpu_temp: Some(51.0),
            cpu_history: (0..30).map(|i| i * 3).collect(),
            mem_used: 4 << 30,
            mem_total: 16 << 30,
            swap_used: 1 << 30,
            swap_total: 8 << 30,
            disks: vec![DiskInfo {
                device: "/dev/nvme0n1p5".into(),
                mount_point: "/".into(),
                fs_type: "btrfs".into(),
                total: 500 << 30,
                used: 100 << 30,
                kind: "SSD".into(),
                removable: false,
            }],
            rx_rate: 1536.0,
            tx_rate: 200.0,
            rx_history: (0..20).map(|i| i * 100).collect(),
            tx_history: (0..20).map(|i| i * 10).collect(),
            load: Some([0.5, 0.25, 0.1]),
            processes: Some(312),
            uptime: Some(3700),
        };
        a
    }

    fn render(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf.get(x, y).symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn live_modules_are_removed_from_static_sections() {
        let secs = static_sections(&Config::default().sections);
        let names: Vec<String> = secs
            .iter()
            .flat_map(|s| s.modules.iter())
            .filter_map(|m| match m {
                ModuleEntry::Name(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        for gone in [
            "memory",
            "swap",
            "disk",
            "cpu_usage",
            "load",
            "uptime",
            "processes",
            "network",
        ] {
            assert!(!names.contains(&gone.to_string()), "{gone}");
        }
        for kept in ["os", "kernel", "cpu", "gpu", "de", "host"] {
            assert!(names.contains(&kept.to_string()), "{kept}");
        }
    }

    #[test]
    fn history_is_capped() {
        let mut h = VecDeque::new();
        for i in 0..100 {
            push_history(&mut h, i, 10);
        }
        assert_eq!(h.len(), 10);
        assert_eq!((h.front(), h.back()), (Some(&90), Some(&99)));
        push_history(&mut h, 1, 0);
        assert_eq!(h.len(), 1, "cap is at least one sample");
    }

    #[test]
    fn rate_formatting() {
        assert_eq!(format_rate(0.0), "0 B/s");
        assert_eq!(format_rate(1536.0), "1.5 KiB/s");
        assert_eq!(format_rate(5.0 * 1024.0 * 1024.0), "5.0 MiB/s");
        assert_eq!(format_rate(-3.0), "0 B/s");
    }

    #[test]
    fn dashboard_shows_all_panels_and_logo() {
        let out = render(&app(true), 110, 46).join("\n");
        for needle in [
            "LOGO",
            "Software",
            "Fedora Linux 41",
            "CPU",
            "42%",
            "51°C",
            "Memory",
            "RAM 4.00 GiB / 16.00 GiB (25%)",
            "swap",
            "Disks",
            "btrfs",
            "Network",
            "↓ 1.5 KiB/s",
            "↑ 200 B/s",
            "up 1 hour",
            "312 procs",
            "q quit",
        ] {
            assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
        }
        assert!(out.contains('╭'), "rounded unicode borders");
    }

    #[test]
    fn ascii_mode_uses_plain_borders() {
        let out = render(&app(false), 110, 46).join("\n");
        assert!(out.contains("+-") && out.contains("| "), "{out}");
        assert!(out.contains("v 1.5 KiB/s") && out.contains("^ 200 B/s"));
        assert!(!out.contains('╭') && !out.contains('↓'));
    }

    #[test]
    fn small_terminals_drop_low_priority_panels() {
        let a = app(true);
        assert!(a.plan(60).iter().any(|(p, _)| *p == Panel::Net));
        let small = a.plan(14);
        let kinds: Vec<Panel> = small.iter().map(|(p, _)| *p).collect();
        assert!(!kinds.contains(&Panel::Net) && !kinds.contains(&Panel::Disks));
        assert!(kinds.contains(&Panel::Cpu) && kinds.contains(&Panel::Mem));
        assert!(small.iter().map(|(_, h)| *h).sum::<u16>() <= 14);
        assert!(a.plan(3).iter().map(|(_, h)| *h).sum::<u16>() <= 3);
        let _ = render(&a, 80, 12); // must not panic
    }

    #[test]
    fn narrow_terminal_hides_logo_and_tiny_one_shows_message() {
        let narrow = render(&app(true), 44, 40).join("\n");
        assert!(!narrow.contains("LOGO"));
        assert!(narrow.contains("CPU"));
        let tiny = render(&app(true), 20, 3).join("\n");
        assert!(tiny.contains("too small"));
    }

    #[test]
    fn theme_cycling_rebuilds_and_wraps() {
        let mut a = app(true);
        assert_eq!(a.theme_name(), "default");
        a.next_theme();
        assert_eq!(a.theme_name(), "ocean");
        for _ in 0..theme::BUILTIN_NAMES.len() - 1 {
            a.next_theme();
        }
        assert_eq!(a.theme_name(), "default", "wraps around");
    }

    #[test]
    fn sampler_fills_live_data_on_this_machine() {
        let mut s = Sampler::new(vec![]);
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL + Duration::from_millis(30));
        let mut live = Live::default();
        s.sample(&mut live, 5);
        assert!(live.mem_total > 0 && live.mem_used <= live.mem_total);
        assert!(!live.cores.is_empty());
        assert!((0.0..=100.0).contains(&live.cpu_total));
        assert_eq!(live.cpu_history.len(), 1);
        s.sample(&mut live, 5);
        assert_eq!(live.cpu_history.len(), 2);
        assert!(live.uptime.is_some());
    }
}
