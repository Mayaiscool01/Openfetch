//! Turning [`SystemInfo`] into styled lines, laying them out beside a logo and
//! rendering them as ANSI text. The same line model feeds the animation engine.

use crate::config::{Config, LogoPosition, ModuleEntry, SectionConfig};
use crate::memory::percent;
use crate::system::{ModuleKind, SystemInfo};
use crate::theme::{ColorDepth, ColorSpec, Theme};
use owo_colors::{AnsiColors, DynColors, OwoColorize, XtermColors};

// ------------------------------------------------------------- line model

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub color: Option<ColorSpec>,
    /// Background colour (used by half-block image rendering).
    pub bg: Option<ColorSpec>,
    pub bold: bool,
}

pub type Line = Vec<Span>;

impl Span {
    pub fn new(text: impl Into<String>, color: ColorSpec) -> Span {
        Span {
            text: text.into(),
            color: Some(color),
            bg: None,
            bold: false,
        }
    }
    pub fn plain(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            color: None,
            bg: None,
            bold: false,
        }
    }
    pub fn bold(mut self) -> Span {
        self.bold = true;
        self
    }
    pub fn on(mut self, bg: ColorSpec) -> Span {
        self.bg = Some(bg);
        self
    }
}

pub fn line_width(line: &Line) -> usize {
    line.iter().map(|s| s.text.chars().count()).sum()
}

// ---------------------------------------------------------------- charset

/// Characters used for bars and decoration; ASCII-only when Unicode is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Charset {
    pub unicode: bool,
}

impl Charset {
    pub fn bar_full(self) -> &'static str {
        if self.unicode {
            "█"
        } else {
            "#"
        }
    }
    pub fn bar_empty(self) -> &'static str {
        if self.unicode {
            "░"
        } else {
            "-"
        }
    }
    pub fn ellipsis(self) -> &'static str {
        if self.unicode {
            "…"
        } else {
            "~"
        }
    }
    pub fn swatch(self) -> &'static str {
        if self.unicode {
            "███"
        } else {
            "###"
        }
    }
    pub fn degrees(self) -> &'static str {
        if self.unicode {
            "°C"
        } else {
            " C"
        }
    }
}

/// UTF-8 locale and not the bare Linux console.
pub fn detect_unicode(get: impl Fn(&str) -> Option<String>) -> bool {
    if get("TERM").as_deref() == Some("linux") {
        return false;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|k| get(k).filter(|v| !v.is_empty()))
        .next()
        .map(|v| {
            let v = v.to_ascii_lowercase();
            v.contains("utf-8") || v.contains("utf8")
        })
        .unwrap_or(false)
}

// ------------------------------------------------------------- formatting

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    match i {
        0 => format!("{bytes} B"),
        1 | 2 => format!("{v:.1} {}", UNITS[i]),
        _ => format!("{v:.2} {}", UNITS[i]),
    }
}

pub fn format_uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    let plural = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(plural(d, "day"));
    }
    if h > 0 {
        parts.push(plural(h, "hour"));
    }
    if m > 0 {
        parts.push(plural(m, "min"));
    }
    if parts.is_empty() {
        "less than a minute".to_string()
    } else {
        parts.join(", ")
    }
}

/// `(filled, empty)` cell counts for a bar of `width` cells.
pub fn bar_cells(pct: f32, width: usize) -> (usize, usize) {
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * width as f32).round() as usize;
    (filled.min(width), width - filled.min(width))
}

pub fn format_frequency(mhz: u32) -> String {
    format!("{:.2} GHz", mhz as f64 / 1000.0)
}

fn format_minutes(m: u32) -> String {
    format!("{}h {:02}m", m / 60, m % 60)
}

// ------------------------------------------------------------ info rows

enum Row {
    Kv { key: String, value: Vec<Span> },
    Title,
    Rule,
    Break,
    Colors,
}

#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub info: &'a SystemInfo,
    pub cfg: &'a Config,
    pub theme: &'a Theme,
    pub cs: Charset,
    pub compact: bool,
    /// Whether rows carry a `Label:`; when false short hints keep values clear.
    pub show_labels: bool,
}

impl Ctx<'_> {
    /// A dim hint such as `up ` placed before a value when labels are hidden.
    fn hint(&self, text: &str) -> Vec<Span> {
        if self.show_labels {
            Vec::new()
        } else {
            vec![Span::new(text, self.theme.separator)]
        }
    }

    fn text(&self, s: impl Into<String>) -> Span {
        Span::new(s, self.theme.value)
    }

    fn label(&self, kind: ModuleKind, default: &str) -> String {
        self.cfg
            .labels
            .get(kind.key())
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    fn level_color(&self, pct: f32) -> ColorSpec {
        if pct >= 90.0 {
            self.theme.critical
        } else if pct >= 70.0 {
            self.theme.warn
        } else {
            self.theme.value
        }
    }

    /// `12%` plus an optional bar, coloured by how full it is.
    fn percent_spans(&self, pct: f32, with_bar: bool) -> Vec<Span> {
        let color = self.level_color(pct);
        let mut spans = vec![Span::new(format!("{pct:.0}%"), color)];
        if with_bar && self.cfg.layout.bars && !self.compact {
            let (full, empty) = bar_cells(pct, self.cfg.layout.bar_width);
            let bar_color = if color == self.theme.value {
                self.theme.bar
            } else {
                color
            };
            spans.push(Span::plain(" "));
            spans.push(Span::new(self.cs.bar_full().repeat(full), bar_color));
            spans.push(Span::new(
                self.cs.bar_empty().repeat(empty),
                self.theme.bar_empty,
            ));
        }
        spans
    }

    fn usage_spans(&self, used: u64, total: u64) -> Vec<Span> {
        let pct = percent(used, total);
        let mut spans = vec![self.text(format!(
            "{} / {} (",
            format_bytes(used),
            format_bytes(total)
        ))];
        let mut p = self.percent_spans(pct, true);
        // the bar goes after the closing parenthesis
        let tail = if p.len() > 1 {
            p.split_off(1)
        } else {
            Vec::new()
        };
        spans.extend(p);
        spans.push(self.text(")"));
        spans.extend(tail);
        spans
    }
}

fn kv(rows: &mut Vec<Row>, key: String, value: Vec<Span>) {
    rows.push(Row::Kv { key, value });
}

fn module_rows(ctx: &Ctx, kind: ModuleKind, rows: &mut Vec<Row>) {
    use ModuleKind::*;
    let info = ctx.info;
    let t = |s: String| vec![ctx.text(s)];
    match kind {
        Title => rows.push(Row::Title),
        Rule => rows.push(Row::Rule),
        Break => rows.push(Row::Break),
        Colors => rows.push(Row::Colors),
        Os => {
            if let Some(os) = &info.os {
                kv(
                    rows,
                    ctx.label(kind, "OS"),
                    t(format!("{} {}", os.pretty_name, os.arch)),
                );
            }
        }
        Host => {
            if let Some(h) = &info.host {
                kv(rows, ctx.label(kind, "Host"), t(h.clone()));
            }
        }
        Kernel => {
            if let Some(k) = &info.kernel {
                kv(
                    rows,
                    ctx.label(kind, "Kernel"),
                    t(format!("{} {k}", crate::os::KERNEL_NAME)),
                );
            }
        }
        Uptime => {
            if let Some(u) = info.uptime_seconds {
                let mut v = ctx.hint("up ");
                v.push(ctx.text(format_uptime(u)));
                kv(rows, ctx.label(kind, "Uptime"), v);
            }
        }
        Packages => {
            if !info.packages.is_empty() {
                let s = info
                    .packages
                    .iter()
                    .map(|p| format!("{} ({})", p.count, p.manager))
                    .collect::<Vec<_>>()
                    .join(", ");
                kv(rows, ctx.label(kind, "Packages"), t(s));
            }
        }
        Shell => {
            if let Some(s) = &info.shell {
                kv(rows, ctx.label(kind, "Shell"), t(s.clone()));
            }
        }
        Terminal => {
            if let Some(s) = &info.terminal {
                kv(rows, ctx.label(kind, "Terminal"), t(s.clone()));
            }
        }
        De => {
            if let Some(de) = info.desktop.as_ref().and_then(|d| d.de.clone()) {
                kv(rows, ctx.label(kind, "DE"), t(de));
            }
        }
        Wm => {
            if let Some(wm) = info.desktop.as_ref().and_then(|d| d.wm_display()) {
                kv(rows, ctx.label(kind, "WM"), t(wm));
            }
        }
        Resolution => {
            for d in &info.displays {
                let head = d.monitor.clone().unwrap_or_else(|| d.name.clone());
                let mut v = format!("{head} {}", d.resolution);
                if let Some(hz) = d.refresh_hz {
                    v.push_str(&format!("@{hz:.2}Hz"));
                }
                if ctx.show_labels && d.monitor.is_some() {
                    v.push_str(&format!(" ({})", d.name));
                }
                kv(rows, ctx.label(kind, "Resolution"), t(v));
            }
        }
        Motherboard => {
            if let Some(m) = &info.motherboard {
                kv(rows, ctx.label(kind, "Motherboard"), t(m.clone()));
            }
        }
        Bios => {
            if let Some(f) = &info.firmware {
                let mut main = [f.vendor.as_deref(), f.version.as_deref()]
                    .iter()
                    .flatten()
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut extra = vec![f.mode.clone()];
                if let Some(sb) = f.secure_boot {
                    extra.push(format!("Secure Boot {}", if sb { "on" } else { "off" }));
                }
                if main.is_empty() {
                    main = f.mode.clone();
                    extra.remove(0);
                }
                let s = if extra.is_empty() {
                    main
                } else {
                    format!("{main} ({})", extra.join(", "))
                };
                kv(rows, ctx.label(kind, "BIOS"), t(s));
            }
        }
        Cpu => {
            if let Some(c) = &info.cpu {
                let mut s = format!("{} ({})", c.model, crate::cpu::topology(c.cores, c.threads));
                // Intel model strings already carry "@ 2.80GHz".
                if let (Some(mhz), false) = (c.max_mhz, c.model.contains(" @ ")) {
                    s.push_str(&format!(" @ {}", format_frequency(mhz)));
                }
                let mut value = vec![ctx.text(s)];
                if ctx.compact {
                    if let Some(u) = c.usage_percent {
                        value.push(ctx.text(" - "));
                        value.extend(ctx.percent_spans(u, false));
                    }
                    if let Some(temp) = c.temperature_c {
                        value.push(ctx.text(format!(" - {temp:.0}{}", ctx.cs.degrees())));
                    }
                }
                kv(rows, ctx.label(kind, "CPU"), value);
            }
        }
        CpuUsage => {
            if let (false, Some(u)) = (ctx.compact, info.cpu.as_ref().and_then(|c| c.usage_percent))
            {
                let mut v = ctx.hint("CPU ");
                v.extend(ctx.percent_spans(u, true));
                kv(rows, ctx.label(kind, "CPU Usage"), v);
            }
        }
        CpuTemp => {
            if let (false, Some(temp)) =
                (ctx.compact, info.cpu.as_ref().and_then(|c| c.temperature_c))
            {
                let color = ctx.level_color((temp - 20.0).max(0.0) / 80.0 * 100.0);
                let mut v = ctx.hint("CPU ");
                v.push(Span::new(format!("{temp:.1}{}", ctx.cs.degrees()), color));
                kv(rows, ctx.label(kind, "CPU Temp"), v);
            }
        }
        Gpu => {
            for g in &info.gpus {
                kv(rows, ctx.label(kind, "GPU"), t(g.name.clone()));
            }
        }
        Memory => {
            if let Some(m) = info.memory.as_ref().filter(|m| m.total > 0) {
                kv(
                    rows,
                    ctx.label(kind, "Memory"),
                    ctx.usage_spans(m.used, m.total),
                );
            }
        }
        Swap => {
            if let Some(m) = info.memory.as_ref().filter(|m| m.swap_total > 0) {
                let mut v = ctx.usage_spans(m.swap_used, m.swap_total);
                if !ctx.show_labels {
                    v.push(Span::new(" (swap)", ctx.theme.separator));
                }
                kv(rows, ctx.label(kind, "Swap"), v);
            }
        }
        Disk => {
            let base = ctx.label(kind, "Disk");
            let disks: Vec<_> = if ctx.compact {
                info.disks
                    .iter()
                    .find(|d| d.mount_point == "/")
                    .or(info.disks.first())
                    .into_iter()
                    .collect()
            } else {
                info.disks.iter().collect()
            };
            for d in disks {
                let mut value = ctx.usage_spans(d.used, d.total);
                let mut note = format!(" - {}", d.fs_type);
                if !d.kind.is_empty() {
                    note.push_str(&format!(", {}", d.kind));
                }
                if d.removable {
                    note.push_str(", removable");
                }
                if !ctx.show_labels {
                    note.push_str(&format!(" ({})", d.mount_point));
                }
                value.push(Span::new(note, ctx.theme.separator));
                kv(rows, format!("{base} ({})", d.mount_point), value);
            }
        }
        Battery => {
            let base = ctx.label(kind, "Battery");
            for (i, b) in info.batteries.iter().enumerate() {
                let key = if info.batteries.len() > 1 {
                    format!("{base} {}", i + 1)
                } else {
                    base.clone()
                };
                let pct = b.percent.map(|p| p as f32);
                let mut value = ctx.hint("battery ");
                match pct {
                    Some(p) => {
                        // low charge is the warning case, so invert the scale
                        let color = ctx.level_color(100.0 - p);
                        value.push(Span::new(format!("{p:.0}%"), color));
                        value.push(ctx.text(format!(" ({})", b.status)));
                    }
                    None => value.push(ctx.text(b.status.clone())),
                }
                if let Some(m) = b.minutes_remaining {
                    value.push(ctx.text(format!(", {} left", format_minutes(m))));
                }
                if !ctx.compact {
                    if let Some(h) = b.health_percent {
                        value.push(Span::new(format!(" - health {h:.0}%"), ctx.theme.separator));
                    }
                    if let Some(c) = b.cycles {
                        value.push(Span::new(format!(", {c} cycles"), ctx.theme.separator));
                    }
                }
                kv(rows, key, value);
            }
        }
        Network => {
            if ctx.compact {
                return;
            }
            let base = ctx.label(kind, "Network");
            if let Some(n) = &info.network {
                for i in n
                    .interfaces
                    .iter()
                    .filter(|i| i.up && !i.addresses.is_empty())
                {
                    let mut v = i.addresses.join(", ");
                    if !ctx.show_labels {
                        v.push_str(&format!(" ({})", i.name));
                    }
                    kv(rows, format!("{base} ({})", i.name), t(v));
                }
            }
        }
        LocalIp => {
            if let Some(n) = &info.network {
                if let Some(ip) = &n.local_ip {
                    let s = match &n.primary {
                        Some(p) => format!("{ip} ({p})"),
                        None => ip.clone(),
                    };
                    let mut v = ctx.hint("local ip ");
                    v.push(ctx.text(s));
                    kv(rows, ctx.label(kind, "Local IP"), v);
                }
            }
        }
        Theme | Icons | Cursor | Font => {
            let Some(a) = &info.appearance else { return };
            let (label, value) = match kind {
                Theme => ("Theme", &a.theme),
                Icons => ("Icons", &a.icons),
                Cursor => ("Cursor", &a.cursor),
                _ => ("Font", &a.font),
            };
            if let Some(v) = value {
                kv(rows, ctx.label(kind, label), t(v.clone()));
            }
        }
        Load => {
            if let Some([a, b, c]) = info.load_average {
                let mut v = ctx.hint("load ");
                v.push(ctx.text(format!("{a:.2}, {b:.2}, {c:.2}")));
                kv(rows, ctx.label(kind, "Load"), v);
            }
        }
        Processes => {
            if let Some(n) = info.processes {
                let mut v = vec![ctx.text(n.to_string())];
                if !ctx.show_labels {
                    v.push(Span::new(" processes", ctx.theme.separator));
                }
                kv(rows, ctx.label(kind, "Processes"), v);
            }
        }
        Init => {
            if let Some(i) = &info.init {
                let mut v = vec![ctx.text(i.clone())];
                if !ctx.show_labels {
                    v.push(Span::new(" (init)", ctx.theme.separator));
                }
                kv(rows, ctx.label(kind, "Init"), v);
            }
        }
        Locale => {
            if let Some(l) = &info.locale {
                let mut v = ctx.hint("locale ");
                v.push(ctx.text(l.clone()));
                kv(rows, ctx.label(kind, "Locale"), v);
            }
        }
    }
}

fn collect_rows(ctx: &Ctx, entries: &[ModuleEntry]) -> Vec<Row> {
    let mut rows = Vec::new();
    for e in entries {
        match e {
            ModuleEntry::Custom { label, text } => {
                kv(&mut rows, label.clone(), vec![ctx.text(text.clone())])
            }
            ModuleEntry::Name(n) => {
                if let Some(kind) = ModuleKind::from_name(n) {
                    module_rows(ctx, kind, &mut rows);
                }
            }
        }
    }
    rows
}

fn title_parts(ctx: &Ctx) -> (String, String) {
    (
        ctx.info.username.clone().unwrap_or_else(|| "user".into()),
        ctx.info
            .hostname
            .clone()
            .unwrap_or_else(|| "localhost".into()),
    )
}

fn swatch_lines(ctx: &Ctx) -> Vec<Line> {
    [0u8, 8]
        .iter()
        .map(|base| {
            (0..8)
                .map(|i| Span::new(ctx.cs.swatch(), ColorSpec::Ansi(base + i)))
                .collect()
        })
        .collect()
}

/// Build the flat (unframed) info column for the given module list.
pub fn build_info(ctx: &Ctx, entries: &[ModuleEntry]) -> Vec<Line> {
    let rows = collect_rows(ctx, entries);
    let (user, host) = title_parts(ctx);
    let title_width = user.chars().count() + 1 + host.chars().count();
    let sep = &ctx.cfg.layout.separator;
    let key_width = if ctx.cfg.layout.key_align {
        rows.iter()
            .filter_map(|r| match r {
                Row::Kv { key, .. } => Some(key.chars().count()),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    let rule_char = ctx.cfg.layout.rule.chars().next().map(|c| {
        if !ctx.cs.unicode && !c.is_ascii() {
            '-'
        } else {
            c
        }
    });

    let mut lines = Vec::new();
    for row in rows {
        match row {
            Row::Title => lines.push(vec![
                Span::new(user.clone(), ctx.theme.title).bold(),
                Span::new("@", ctx.theme.separator),
                Span::new(host.clone(), ctx.theme.title).bold(),
            ]),
            Row::Rule => {
                if let Some(c) = rule_char {
                    lines.push(vec![Span::new(
                        c.to_string().repeat(title_width),
                        ctx.theme.rule,
                    )]);
                }
            }
            Row::Break => lines.push(Vec::new()),
            Row::Colors => lines.extend(swatch_lines(ctx)),
            Row::Kv { key, value } => {
                let used = key.chars().count() + sep.chars().count();
                let pad = (key_width + sep.chars().count()).saturating_sub(used) + 1;
                let mut line = vec![
                    Span::new(key, ctx.theme.key).bold(),
                    Span::new(sep.clone(), ctx.theme.separator),
                    Span::plain(" ".repeat(pad)),
                ];
                line.extend(value);
                lines.push(line);
            }
        }
    }
    lines
}

/// Content lines of one panel: `-> value` rows (or `-> Label: value`).
fn panel_content(ctx: &Ctx, entries: &[ModuleEntry], bullet: &str) -> Vec<Line> {
    let rows = collect_rows(ctx, entries);
    let (user, host) = title_parts(ctx);
    let sep = &ctx.cfg.layout.separator;
    let key_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Kv { key, .. } => Some(key.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    for row in rows {
        match row {
            Row::Title => lines.push(vec![
                Span::new(user.clone(), ctx.theme.title).bold(),
                Span::new("@", ctx.theme.separator),
                Span::new(host.clone(), ctx.theme.title).bold(),
            ]),
            Row::Rule => {}
            Row::Break => lines.push(Vec::new()),
            Row::Colors => lines.extend(swatch_lines(ctx)),
            Row::Kv { key, value } => {
                let mut line = vec![Span::new(format!("{bullet} "), ctx.theme.key)];
                if ctx.show_labels {
                    let pad = key_width.saturating_sub(key.chars().count());
                    line.push(Span::new(key, ctx.theme.key).bold());
                    line.push(Span::new(sep.clone(), ctx.theme.separator));
                    line.push(Span::plain(" ".repeat(pad + 1)));
                }
                line.extend(value);
                lines.push(line);
            }
        }
    }
    // a section never starts or ends with a gap
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// Rows of one `[[sections]]` entry, without borders (`→ value`).
pub fn section_lines(ctx: &Ctx, sec: &SectionConfig) -> Vec<Line> {
    let sctx = Ctx {
        show_labels: sec.labels.unwrap_or(ctx.cfg.layout.show_labels),
        ..*ctx
    };
    let bullet = sec.bullet.clone().unwrap_or_else(|| {
        let b = ctx.cfg.layout.bullet.clone();
        if !ctx.cs.unicode && !b.is_ascii() {
            "->".to_string()
        } else {
            b
        }
    });
    panel_content(&sctx, &sec.modules, &bullet)
}

/// Build titled, bordered panels from `[[sections]]`. `max_width` is the room
/// available for the whole column (`None` = unlimited).
pub fn build_panels(ctx: &Ctx, sections: &[SectionConfig], max_width: Option<usize>) -> Vec<Line> {
    let cs = ctx.cs;
    let (tl, tr, bl, br, h, v) = if cs.unicode {
        ('╭', '╮', '╰', '╯', '─', '│')
    } else {
        ('+', '+', '+', '+', '-', '|')
    };
    let mut built: Vec<(String, Vec<Line>)> = Vec::new();
    for sec in sections {
        let content = section_lines(ctx, sec);
        if content.iter().any(|l| !l.is_empty()) {
            built.push((sec.title.clone(), content));
        }
    }

    let natural = built
        .iter()
        .flat_map(|(t, c)| std::iter::once(t.chars().count() + 2).chain(c.iter().map(line_width)))
        .max()
        .unwrap_or(0);
    let inner = match max_width {
        Some(m) => natural.min(m.saturating_sub(4)),
        None => natural,
    };
    let total = inner + 4;
    let border = |text: String| Span::new(text, ctx.theme.rule);

    let mut out = Vec::new();
    for (title, content) in built {
        let title: String = title.chars().take(inner.saturating_sub(2)).collect();
        let title_w = title.chars().count();
        let mut top = vec![border(format!("{tl}{h} "))];
        if title_w > 0 {
            top.push(Span::new(title, ctx.theme.title).bold());
            top.push(border(" ".into()));
        }
        let used = 3 + if title_w > 0 { title_w + 1 } else { 0 };
        top.push(border(format!(
            "{}{tr}",
            h.to_string().repeat(total.saturating_sub(used + 1).max(1))
        )));
        out.push(top);
        for line in content {
            let line = truncate_line(&line, inner, cs.ellipsis());
            let mut l = vec![border(format!("{v} "))];
            let w = line_width(&line);
            l.extend(line);
            l.push(Span::plain(" ".repeat(inner - w)));
            l.push(border(format!(" {v}")));
            out.push(l);
        }
        out.push(vec![border(format!(
            "{bl}{}{br}",
            h.to_string().repeat(total - 2)
        ))]);
    }
    out
}

// ----------------------------------------------------------------- layout

pub struct ComposeOpts {
    pub position: LogoPosition,
    pub gap: usize,
    pub margin: usize,
    pub max_width: Option<usize>,
    pub ellipsis: &'static str,
}

/// The logo is dropped when less than this much room is left for the text.
pub const MIN_INFO_WIDTH: usize = 30;

pub fn truncate_line(line: &Line, max: usize, ellipsis: &str) -> Line {
    if line_width(line) <= max {
        return line.clone();
    }
    let keep = max.saturating_sub(ellipsis.chars().count());
    let mut out: Line = Vec::new();
    let mut left = keep;
    for span in line {
        if left == 0 {
            break;
        }
        let n = span.text.chars().count();
        let mut s = span.clone();
        if n > left {
            s.text = span.text.chars().take(left).collect();
        }
        left -= s.text.chars().count();
        out.push(s);
    }
    let style = out.last().cloned().unwrap_or_else(|| Span::plain(""));
    out.push(Span {
        text: ellipsis.to_string(),
        ..style
    });
    out
}

fn pad(line: &mut Line, from: usize, to: usize) {
    if to > from {
        line.push(Span::plain(" ".repeat(to - from)));
    }
}

/// Whether a logo of width `logo_w` fits beside the info column.
pub fn logo_visible(logo_w: usize, o: &ComposeOpts) -> bool {
    o.position != LogoPosition::None
        && logo_w > 0
        && o.max_width
            .map_or(true, |m| o.margin + logo_w + o.gap + MIN_INFO_WIDTH <= m)
}

/// Columns left for the info column (`None` = unlimited).
pub fn info_budget(logo_w: usize, o: &ComposeOpts) -> Option<usize> {
    let used = o.margin
        + if logo_visible(logo_w, o) {
            logo_w + o.gap
        } else {
            0
        };
    o.max_width.map(|m| m.saturating_sub(used))
}

/// Place the info column beside the logo, respecting the terminal width.
pub fn compose(logo: &[Line], info: &[Line], o: &ComposeOpts) -> Vec<Line> {
    let logo_w = logo.iter().map(line_width).max().unwrap_or(0);
    let show_logo = logo_visible(logo_w, o);
    let avail = info_budget(logo_w, o);
    let info: Vec<Line> = info
        .iter()
        .map(|l| match avail {
            Some(a) => truncate_line(l, a, o.ellipsis),
            None => l.clone(),
        })
        .collect();
    let info_w = info.iter().map(line_width).max().unwrap_or(0);
    let rows = if show_logo {
        logo.len().max(info.len())
    } else {
        info.len()
    };
    let empty = Line::new();

    (0..rows)
        .map(|i| {
            let mut line: Line = Vec::new();
            pad(&mut line, 0, o.margin);
            let l = logo.get(i).unwrap_or(&empty);
            let r = info.get(i).unwrap_or(&empty);
            if !show_logo {
                line.extend(r.iter().cloned());
            } else if o.position == LogoPosition::Right {
                line.extend(r.iter().cloned());
                pad(&mut line, line_width(r), info_w + o.gap);
                line.extend(l.iter().cloned());
            } else {
                line.extend(l.iter().cloned());
                pad(&mut line, line_width(l), logo_w + o.gap);
                line.extend(r.iter().cloned());
            }
            line
        })
        .collect()
}

// -------------------------------------------------------------- rendering

fn ansi_color(n: u8) -> AnsiColors {
    use AnsiColors::*;
    [
        Black,
        Red,
        Green,
        Yellow,
        Blue,
        Magenta,
        Cyan,
        White,
        BrightBlack,
        BrightRed,
        BrightGreen,
        BrightYellow,
        BrightBlue,
        BrightMagenta,
        BrightCyan,
        BrightWhite,
    ][(n as usize) % 16]
}

fn dyn_color(c: ColorSpec) -> DynColors {
    match c {
        ColorSpec::Ansi(n) => DynColors::Ansi(ansi_color(n)),
        ColorSpec::Indexed(n) => DynColors::Xterm(XtermColors::from(n)),
        ColorSpec::Rgb(r, g, b) => DynColors::Rgb(r, g, b),
    }
}

/// Render lines as text with ANSI escapes (`depth == None` gives plain text).
pub fn render_ansi(lines: &[Line], depth: ColorDepth) -> String {
    let mut out = String::new();
    for line in lines {
        // drop trailing blanks so plain output stays clean
        let mut spans: Vec<&Span> = line.iter().collect();
        while spans
            .last()
            .is_some_and(|s| s.color.is_none() && s.text.trim().is_empty())
        {
            spans.pop();
        }
        for s in spans {
            if depth == ColorDepth::None || (s.color.is_none() && s.bg.is_none() && !s.bold) {
                out.push_str(&s.text);
                continue;
            }
            let fg = s.color.map(|c| dyn_color(c.downgrade(depth)));
            let bg = s.bg.map(|c| dyn_color(c.downgrade(depth)));
            let text = s.text.as_str();
            let styled = match (fg, bg, s.bold) {
                (Some(f), Some(b), true) => text.color(f).on_color(b).bold().to_string(),
                (Some(f), Some(b), false) => text.color(f).on_color(b).to_string(),
                (Some(f), None, true) => text.color(f).bold().to_string(),
                (Some(f), None, false) => text.color(f).to_string(),
                (None, Some(b), true) => text.on_color(b).bold().to_string(),
                (None, Some(b), false) => text.on_color(b).to_string(),
                (None, None, _) => text.bold().to_string(),
            };
            out.push_str(&styled);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
pub fn render_plain(lines: &[Line]) -> String {
    render_ansi(lines, ColorDepth::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::CpuInfo;
    use crate::memory::MemoryInfo;
    use crate::os::OsRelease;
    use crate::storage::DiskInfo;

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect()
    }

    fn sample() -> SystemInfo {
        SystemInfo {
            username: Some("mia".into()),
            hostname: Some("asahi".into()),
            os: Some(OsRelease {
                pretty_name: "Fedora Linux 41 (Asahi Remix)".into(),
                arch: "aarch64".into(),
                ..Default::default()
            }),
            kernel: Some("6.11.0".into()),
            uptime_seconds: Some(93_784),
            cpu: Some(CpuInfo {
                model: "Apple M1".into(),
                cores: Some(8),
                threads: 8,
                max_mhz: Some(3200),
                usage_percent: Some(12.4),
                temperature_c: None,
            }),
            memory: Some(MemoryInfo {
                total: 16 << 30,
                used: 4 << 30,
                swap_total: 0,
                swap_used: 0,
            }),
            disks: vec![DiskInfo {
                device: "/dev/nvme0n1p5".into(),
                mount_point: "/".into(),
                fs_type: "btrfs".into(),
                total: 500 << 30,
                used: 100 << 30,
                kind: "SSD".into(),
                removable: false,
            }],
            ..Default::default()
        }
    }

    fn entries(names: &[&str]) -> Vec<ModuleEntry> {
        names
            .iter()
            .map(|n| ModuleEntry::Name(n.to_string()))
            .collect()
    }

    #[test]
    fn formats_bytes_and_uptime() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(5 << 30), "5.00 GiB");
        assert_eq!(format_uptime(30), "less than a minute");
        assert_eq!(format_uptime(93_784), "1 day, 2 hours, 3 mins");
        assert_eq!(format_uptime(3600), "1 hour");
    }

    #[test]
    fn bar_math() {
        assert_eq!(bar_cells(0.0, 10), (0, 10));
        assert_eq!(bar_cells(50.0, 10), (5, 5));
        assert_eq!(bar_cells(150.0, 10), (10, 0));
        assert_eq!(bar_cells(-5.0, 10), (0, 10));
    }

    #[test]
    fn unicode_detection() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(detect_unicode(env(&[("LANG", "en_US.UTF-8")])));
        assert!(!detect_unicode(env(&[("LANG", "C")])));
        assert!(!detect_unicode(env(&[])));
        assert!(!detect_unicode(env(&[
            ("LANG", "en_US.UTF-8"),
            ("TERM", "linux")
        ])));
        // LC_ALL wins over LANG
        assert!(!detect_unicode(env(&[
            ("LC_ALL", "C"),
            ("LANG", "en_US.UTF-8")
        ])));
    }

    #[test]
    fn builds_aligned_rows() {
        let mut cfg = Config::default();
        cfg.layout.bars = true;
        let theme = Theme::default();
        let info = sample();
        let ctx = Ctx {
            info: &info,
            cfg: &cfg,
            theme: &theme,
            cs: Charset { unicode: true },
            compact: false,
            show_labels: true,
        };
        let lines = text(&build_info(
            &ctx,
            &entries(&["title", "rule", "os", "kernel", "uptime", "memory"]),
        ));
        assert_eq!(lines[0], "mia@asahi");
        assert_eq!(lines[1], "─────────");
        assert_eq!(lines[2], "OS:     Fedora Linux 41 (Asahi Remix) aarch64");
        assert_eq!(lines[3], "Kernel: Linux 6.11.0");
        assert_eq!(lines[4], "Uptime: 1 day, 2 hours, 3 mins");
        assert!(
            lines[5].starts_with("Memory: 4.00 GiB / 16.00 GiB (25%) ███"),
            "{}",
            lines[5]
        );
    }

    #[test]
    fn ascii_mode_and_labels() {
        let mut cfg = Config::default();
        cfg.labels.insert("os".into(), "Distro".into());
        cfg.layout.key_align = false;
        cfg.layout.bars = true;
        let theme = Theme::default();
        let info = sample();
        let ctx = Ctx {
            info: &info,
            cfg: &cfg,
            theme: &theme,
            cs: Charset { unicode: false },
            compact: false,
            show_labels: true,
        };
        let lines = text(&build_info(&ctx, &entries(&["rule", "os", "memory"])));
        assert_eq!(lines[0], "---------");
        assert!(lines[1].starts_with("Distro: Fedora"));
        assert!(lines[2].contains("###"), "{}", lines[2]);
        assert!(lines.iter().all(|l| l.is_ascii()));
    }

    #[test]
    fn compact_cpu_is_inline_and_swap_hides_when_absent() {
        let (cfg, theme) = (Config::default(), Theme::default());
        let info = sample();
        let ctx = Ctx {
            info: &info,
            cfg: &cfg,
            theme: &theme,
            cs: Charset { unicode: true },
            compact: true,
            show_labels: true,
        };
        let lines = text(&build_info(
            &ctx,
            &entries(&["cpu", "cpu_usage", "swap", "disk"]),
        ));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "CPU:      Apple M1 (8) @ 3.20 GHz - 12%");
        assert!(lines[1].starts_with("Disk (/):"), "{}", lines[1]);
        assert!(!lines[1].contains('█'));
    }

    #[test]
    fn unknown_modules_and_missing_data_are_skipped() {
        let (cfg, theme) = (Config::default(), Theme::default());
        let info = SystemInfo::default();
        let ctx = Ctx {
            info: &info,
            cfg: &cfg,
            theme: &theme,
            cs: Charset { unicode: true },
            compact: false,
            show_labels: true,
        };
        let mut e = entries(&["nonsense", "gpu", "battery", "os"]);
        e.push(ModuleEntry::Custom {
            label: "Motto".into(),
            text: "hi".into(),
        });
        let lines = text(&build_info(&ctx, &e));
        assert_eq!(lines, vec!["Motto: hi".to_string()]);
    }

    #[test]
    fn truncation_keeps_colour_and_adds_ellipsis() {
        let line = vec![Span::new("abcdef", ColorSpec::Ansi(1)), Span::plain("ghij")];
        let t = truncate_line(&line, 8, "~");
        assert_eq!(text(std::slice::from_ref(&t))[0], "abcdefg~");
        assert_eq!(truncate_line(&line, 20, "~"), line);
        let t = truncate_line(&line, 3, "…");
        assert_eq!(text(&[t])[0], "ab…");
    }

    fn opts(position: LogoPosition, max_width: Option<usize>) -> ComposeOpts {
        ComposeOpts {
            position,
            gap: 2,
            margin: 1,
            max_width,
            ellipsis: "~",
        }
    }

    #[test]
    fn composes_logo_left_and_right() {
        let logo = vec![vec![Span::plain("AA")], vec![Span::plain("BB")]];
        let info = vec![
            vec![Span::plain("one")],
            vec![Span::plain("two")],
            vec![Span::plain("three")],
        ];
        let left = text(&compose(&logo, &info, &opts(LogoPosition::Left, None)));
        assert_eq!(left, vec![" AA  one", " BB  two", "     three"]);
        let right = text(&compose(&logo, &info, &opts(LogoPosition::Right, None)));
        assert_eq!(right[0], " one    AA");
        assert_eq!(right[2], " three  ");
        let none = text(&compose(&logo, &info, &opts(LogoPosition::None, None)));
        assert_eq!(none[0], " one");
    }

    #[test]
    fn logo_is_hidden_when_terminal_is_narrow_and_text_is_truncated() {
        let logo = vec![vec![Span::plain("L".repeat(20))]];
        let long = "x".repeat(60);
        let info = vec![vec![Span::plain(long)]];
        let wide = compose(&logo, &info, &opts(LogoPosition::Left, Some(100)));
        assert!(text(&wide)[0].starts_with(" LLLL"));
        let narrow = text(&compose(&logo, &info, &opts(LogoPosition::Left, Some(40))));
        assert!(!narrow[0].contains('L'));
        assert_eq!(narrow[0].chars().count(), 40);
        assert!(narrow[0].ends_with('~'));
    }

    #[test]
    fn ansi_rendering() {
        let lines = vec![vec![
            Span::new("hi", ColorSpec::Ansi(1)).bold(),
            Span::plain("  "),
        ]];
        assert_eq!(render_plain(&lines), "hi\n");
        let colored = render_ansi(&lines, ColorDepth::Ansi256);
        assert!(colored.contains("\u{1b}["), "{colored:?}");
        assert!(colored.contains("hi"));
        let rgb = render_ansi(
            &[vec![Span::new("x", ColorSpec::Rgb(255, 0, 0))]],
            ColorDepth::TrueColor,
        );
        assert!(rgb.contains("38;2;255;0;0"), "{rgb:?}");
        let down = render_ansi(
            &[vec![Span::new("x", ColorSpec::Rgb(255, 0, 0))]],
            ColorDepth::Ansi256,
        );
        assert!(down.contains("38;5;196"), "{down:?}");
    }
    use crate::config::SectionConfig;
    use crate::desktop::{Appearance, Display};

    fn panel_info() -> SystemInfo {
        let mut i = sample();
        i.appearance = Some(Appearance {
            theme: Some("MaterialYou".into()),
            icons: None,
            cursor: Some("Adwaita (24px)".into()),
            font: Some("Noto Sans Mono (11pt)".into()),
        });
        i.displays = vec![Display {
            name: "eDP-1".into(),
            monitor: Some("NCP004D".into()),
            resolution: "1920x1080".into(),
            refresh_hz: Some(141.679),
        }];
        i.load_average = Some([0.5, 0.25, 0.1]);
        i.processes = Some(312);
        i.init = Some("systemd".into());
        i
    }

    fn section(title: &str, mods: &[&str]) -> SectionConfig {
        SectionConfig {
            title: title.into(),
            modules: entries(mods),
            bullet: None,
            labels: None,
        }
    }

    fn panel_ctx<'a>(
        info: &'a SystemInfo,
        cfg: &'a Config,
        theme: &'a Theme,
        unicode: bool,
    ) -> Ctx<'a> {
        Ctx {
            info,
            cfg,
            theme,
            cs: Charset { unicode },
            compact: false,
            show_labels: false,
        }
    }

    #[test]
    fn panels_have_titles_borders_bullets_and_no_labels() {
        let (cfg, theme, info) = (Config::default(), Theme::default(), panel_info());
        let ctx = panel_ctx(&info, &cfg, &theme, true);
        let secs = vec![
            section(
                "Software",
                &["os", "kernel", "break", "theme", "icons", "cursor", "font"],
            ),
            section(
                "Hardware",
                &["resolution", "swap", "load", "processes", "init", "uptime"],
            ),
        ];
        let lines = text(&build_panels(&ctx, &secs, None));
        let w = lines[0].chars().count();
        assert!(
            lines.iter().all(|l| l.chars().count() == w),
            "all lines are equally wide"
        );
        assert!(lines[0].starts_with("╭─ Software ──") && lines[0].ends_with('╮'));
        assert!(
            lines[1].starts_with("│ → Ubuntu") || lines[1].starts_with("│ → Fedora"),
            "{}",
            lines[1]
        );
        assert!(lines[2].starts_with("│ → Linux 6.11.0"));
        assert!(
            lines[3].trim_matches(|c| c == '│' || c == ' ').is_empty(),
            "blank row inside a box"
        );
        assert!(lines.iter().any(|l| l.contains("→ MaterialYou")));
        assert!(
            !lines.iter().any(|l| l.contains("Icons")),
            "unset values are skipped"
        );
        assert!(lines.iter().any(|l| l.contains("→ Adwaita (24px)")));
        assert!(lines.iter().any(|l| l.contains("╭─ Hardware ─")));
        assert!(lines
            .iter()
            .any(|l| l.contains("→ NCP004D 1920x1080@141.68Hz")));
        assert!(lines.iter().any(|l| l.contains("→ load 0.50, 0.25, 0.10")));
        assert!(lines.iter().any(|l| l.contains("→ 312 processes")));
        assert!(lines.iter().any(|l| l.contains("→ systemd (init)")));
        assert!(lines.iter().any(|l| l.contains("→ up 1 day")));
        assert!(lines.last().unwrap().starts_with('╰') && lines.last().unwrap().ends_with('╯'));
    }

    #[test]
    fn panels_fall_back_to_ascii_and_can_show_labels() {
        let (cfg, theme, info) = (Config::default(), Theme::default(), panel_info());
        let ctx = panel_ctx(&info, &cfg, &theme, false);
        let mut sec = section("box", &["kernel", "uptime"]);
        sec.labels = Some(true);
        let lines = text(&build_panels(&ctx, &[sec], None));
        assert!(lines.iter().all(|l| l.is_ascii()), "{lines:?}");
        assert!(lines[0].starts_with("+- box -"));
        assert!(lines[1].starts_with("| -> Kernel: Linux"), "{}", lines[1]);
        assert!(lines[2].starts_with("| -> Uptime: 1 day"), "{}", lines[2]);
    }

    #[test]
    fn panels_respect_width_budget_and_skip_empty_sections() {
        let (cfg, theme, info) = (Config::default(), Theme::default(), panel_info());
        let ctx = panel_ctx(&info, &cfg, &theme, true);
        let secs = vec![
            section("empty", &["gpu", "battery"]),
            section("one", &["os"]),
        ];
        let lines = text(&build_panels(&ctx, &secs, Some(24)));
        assert!(lines.iter().all(|l| l.chars().count() == 24), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("empty")));
        assert!(lines[1].contains('…'));
    }

    #[test]
    fn custom_section_title_and_bullet() {
        let (cfg, theme, info) = (Config::default(), Theme::default(), panel_info());
        let ctx = panel_ctx(&info, &cfg, &theme, true);
        let mut sec = section("My Machine", &["kernel"]);
        sec.bullet = Some("*".into());
        let lines = text(&build_panels(&ctx, &[sec], None));
        assert!(lines[0].contains("My Machine"));
        assert!(lines[1].starts_with("│ * Linux"));
    }
}
