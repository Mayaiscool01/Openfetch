//! Desktop environment, window manager / compositor and display resolution.

use serde::Serialize;
use std::fs;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DesktopInfo {
    pub de: Option<String>,
    pub wm: Option<String>,
    pub session_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Display {
    /// DRM connector, e.g. `eDP-1`.
    pub name: String,
    /// Monitor model from EDID, when available.
    pub monitor: Option<String>,
    pub resolution: String,
    pub refresh_hz: Option<f32>,
}

/// Theme, icons, cursor and font of the desktop session.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Appearance {
    pub theme: Option<String>,
    pub icons: Option<String>,
    pub cursor: Option<String>,
    pub font: Option<String>,
}

type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

fn nonempty(get: Env, key: &str) -> Option<String> {
    get(key).filter(|v| !v.is_empty())
}

/// Names we recognise in `XDG_CURRENT_DESKTOP`, with a nicer spelling.
fn known_de(raw: &str) -> Option<&'static str> {
    Some(match raw.to_ascii_lowercase().as_str() {
        "gnome" | "gnome-classic" | "gnome-xorg" => "GNOME",
        "kde" | "plasma" | "plasmawayland" => "KDE Plasma",
        "x-cinnamon" | "cinnamon" => "Cinnamon",
        "xfce" => "Xfce",
        "mate" => "MATE",
        "lxqt" => "LXQt",
        "lxde" => "LXDE",
        "budgie" => "Budgie",
        "pantheon" => "Pantheon",
        "cosmic" => "COSMIC",
        "deepin" | "dde" => "Deepin",
        "unity" => "Unity",
        "hyprland" => "Hyprland",
        "sway" => "Sway",
        _ => return None,
    })
}

pub fn detect_de(get: Env) -> Option<String> {
    let raw = nonempty(get, "XDG_CURRENT_DESKTOP").or_else(|| nonempty(get, "DESKTOP_SESSION"))?;
    // "ubuntu:GNOME" -> prefer the last component that we know how to name.
    let parts: Vec<&str> = raw.split(':').collect();
    let name = parts
        .iter()
        .rev()
        .find_map(|p| known_de(p))
        .map(str::to_string)
        .unwrap_or_else(|| parts.first().copied().unwrap_or(&raw).to_string());
    if name == "KDE Plasma" {
        if let Some(v) = nonempty(get, "KDE_SESSION_VERSION") {
            return Some(format!("KDE Plasma {v}"));
        }
    }
    Some(name)
}

/// Process names (from `/proc/*/comm`) mapped to a window manager name.
/// Earlier entries win when several are running.
const WM_PROCESSES: &[(&str, &str)] = &[
    ("Hyprland", "Hyprland"),
    ("sway", "Sway"),
    ("niri", "niri"),
    ("river", "River"),
    ("labwc", "labwc"),
    ("wayfire", "Wayfire"),
    ("cosmic-comp", "cosmic-comp"),
    ("kwin_wayland", "KWin"),
    ("kwin_x11", "KWin"),
    ("gnome-shell", "Mutter"),
    ("mutter", "Mutter"),
    ("muffin", "Muffin"),
    ("marco", "Marco"),
    ("xfwm4", "Xfwm4"),
    ("budgie-wm", "Budgie WM"),
    ("i3", "i3"),
    ("bspwm", "bspwm"),
    ("awesome", "awesome"),
    ("qtile", "Qtile"),
    ("herbstluftwm", "herbstluftwm"),
    ("xmonad", "xmonad"),
    ("dwm", "dwm"),
    ("openbox", "Openbox"),
    ("fluxbox", "Fluxbox"),
    ("icewm", "IceWM"),
    ("weston", "Weston"),
    ("gamescope", "gamescope"),
];

/// Choose a WM given the set of running process names.
pub fn wm_from_processes<S: AsRef<str>>(comms: &[S]) -> Option<&'static str> {
    WM_PROCESSES
        .iter()
        .find(|(proc, _)| comms.iter().any(|c| c.as_ref() == *proc))
        .map(|(_, name)| *name)
}

fn running_process_names() -> Vec<String> {
    let Ok(dir) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|e| fs::read_to_string(e.path().join("comm")).ok())
        .map(|s| s.trim().to_string())
        .collect()
}

pub fn detect_wm(get: Env, de: Option<&str>) -> Option<String> {
    if nonempty(get, "HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return Some("Hyprland".into());
    }
    if nonempty(get, "SWAYSOCK").is_some() {
        return Some("Sway".into());
    }
    if nonempty(get, "NIRI_SOCKET").is_some() {
        return Some("niri".into());
    }
    if nonempty(get, "I3SOCK").is_some() {
        return Some("i3".into());
    }
    let from_de = match de.unwrap_or("") {
        d if d.starts_with("GNOME") || d == "Unity" => Some("Mutter"),
        d if d.starts_with("KDE") => Some("KWin"),
        "Xfce" => Some("Xfwm4"),
        "Cinnamon" => Some("Muffin"),
        "MATE" => Some("Marco"),
        "COSMIC" => Some("cosmic-comp"),
        "Budgie" => Some("Mutter"),
        _ => None,
    };
    if let Some(wm) = from_de {
        return Some(wm.into());
    }
    nonempty(get, "XDG_SESSION_TYPE").filter(|t| t != "tty")?;
    wm_from_processes(&running_process_names()).map(str::to_string)
}

pub fn detect(get: Env) -> DesktopInfo {
    if cfg!(target_os = "macos") {
        return DesktopInfo {
            de: Some("Aqua".to_string()),
            wm: Some("Quartz Compositor".to_string()),
            session_type: None,
        };
    }
    let de = detect_de(get);
    let wm = detect_wm(get, de.as_deref());
    let session_type = nonempty(get, "XDG_SESSION_TYPE").map(|t| match t.as_str() {
        "wayland" => "Wayland".to_string(),
        "x11" => "X11".to_string(),
        other => other.to_string(),
    });
    DesktopInfo {
        de,
        wm,
        session_type,
    }
}

impl DesktopInfo {
    /// `Mutter (Wayland)`.
    pub fn wm_display(&self) -> Option<String> {
        let wm = self.wm.as_ref()?;
        Some(match &self.session_type {
            Some(s) if s != "tty" => format!("{wm} ({s})"),
            _ => wm.clone(),
        })
    }
}

/// Strip the `cardN-` prefix of a DRM connector directory name.
fn connector_name(dir: &str) -> Option<&str> {
    let rest = dir.strip_prefix("card")?;
    let (digits, name) = rest.split_once('-')?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some(name)
}

/// Resolution of connected displays, from DRM `modes` (first = preferred mode).
pub fn displays() -> Vec<Display> {
    if cfg!(target_os = "macos") {
        return crate::macos::displays();
    }
    let mut out = Vec::new();
    if let Ok(dir) = fs::read_dir("/sys/class/drm") {
        let mut entries: Vec<_> = dir.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let fname = e.file_name().to_string_lossy().into_owned();
            let Some(name) = connector_name(&fname) else {
                continue;
            };
            let p = e.path();
            let connected = fs::read_to_string(p.join("status"))
                .map(|s| s.trim() == "connected")
                .unwrap_or(false);
            let enabled = fs::read_to_string(p.join("enabled"))
                .map(|s| s.trim() != "disabled")
                .unwrap_or(true);
            if !connected || !enabled {
                continue;
            }
            let mode = fs::read_to_string(p.join("modes"))
                .ok()
                .and_then(|m| m.lines().next().map(|l| l.trim().to_string()))
                .filter(|m| !m.is_empty());
            if let Some(resolution) = mode {
                let edid = fs::read(p.join("edid")).ok().and_then(|b| parse_edid(&b));
                // EDID only tells us the preferred timing, so use its refresh
                // rate only when it matches the mode we report.
                let refresh_hz = edid
                    .as_ref()
                    .and_then(|e| e.preferred)
                    .filter(|(w, h, _)| format!("{w}x{h}") == resolution)
                    .map(|(_, _, hz)| hz);
                out.push(Display {
                    name: name.to_string(),
                    monitor: edid.and_then(|e| e.monitor),
                    resolution,
                    refresh_hz,
                });
            }
        }
    }
    if out.is_empty() {
        if let Ok(size) = fs::read_to_string("/sys/class/graphics/fb0/virtual_size") {
            if let Some((w, h)) = size.trim().split_once(',') {
                out.push(Display {
                    name: "fb0".into(),
                    monitor: None,
                    resolution: format!("{w}x{h}"),
                    refresh_hz: None,
                });
            }
        }
    }
    out
}

// ------------------------------------------------------------------- EDID

#[derive(Debug, Clone, PartialEq)]
pub struct Edid {
    pub monitor: Option<String>,
    /// Width, height and refresh rate of the first (preferred) timing.
    pub preferred: Option<(u32, u32, f32)>,
}

/// Parse the base EDID block: monitor name and preferred timing.
pub fn parse_edid(b: &[u8]) -> Option<Edid> {
    const HEADER: [u8; 8] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];
    if b.len() < 128 || b[..8] != HEADER {
        return None;
    }
    let mut monitor = None;
    let mut preferred = None;
    for off in [54usize, 72, 90, 108] {
        let d = &b[off..off + 18];
        if d[0] != 0 || d[1] != 0 {
            if preferred.is_none() {
                let clock_hz = u16::from_le_bytes([d[0], d[1]]) as f64 * 10_000.0;
                let hactive = d[2] as u32 | ((d[4] as u32 & 0xf0) << 4);
                let hblank = d[3] as u32 | ((d[4] as u32 & 0x0f) << 8);
                let vactive = d[5] as u32 | ((d[7] as u32 & 0xf0) << 4);
                let vblank = d[6] as u32 | ((d[7] as u32 & 0x0f) << 8);
                let total = (hactive + hblank) as f64 * (vactive + vblank) as f64;
                if total > 0.0 && hactive > 0 && vactive > 0 {
                    preferred = Some((hactive, vactive, (clock_hz / total) as f32));
                }
            }
        } else if d[3] == 0xfc && monitor.is_none() {
            let text: String = d[5..18]
                .iter()
                .take_while(|c| **c != b'\n')
                .map(|c| *c as char)
                .collect();
            let text = text.trim().to_string();
            if !text.is_empty() {
                monitor = Some(text);
            }
        }
    }
    Some(Edid { monitor, preferred })
}

// -------------------------------------------------------------- appearance

/// Value of `key` inside `[section]` of INI-style text.
pub fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = "";
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name;
        } else if current == section {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    let v = v.trim().trim_matches('"').trim_matches('\'').trim();
                    return (!v.is_empty()).then(|| v.to_string());
                }
            }
        }
    }
    None
}

/// `Noto Sans Mono 11` (Pango) -> `Noto Sans Mono (11pt)`.
pub fn pango_font(s: &str) -> String {
    match s.rsplit_once(' ') {
        Some((name, size)) if size.parse::<f32>().is_ok() => format!("{name} ({size}pt)"),
        _ => s.to_string(),
    }
}

/// `Noto Sans,10,-1,5,50,0,0,0,0,0` (Qt) -> `Noto Sans (10pt)`.
pub fn qt_font(s: &str) -> String {
    let mut parts = s.split(',');
    match (parts.next(), parts.next()) {
        (Some(name), Some(size)) if size.trim().parse::<f32>().is_ok() => {
            format!("{} ({}pt)", name.trim(), size.trim())
        }
        _ => s.to_string(),
    }
}

fn with_size(name: String, size: Option<String>) -> String {
    match size {
        Some(sz) if sz.parse::<u32>().is_ok_and(|n| n > 0) => format!("{name} ({sz}px)"),
        _ => name,
    }
}

fn gsettings(key: &str) -> Option<String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", key])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_matches('\'')
        .to_string();
    (!v.is_empty()).then_some(v)
}

pub fn detect_appearance(home: &std::path::Path, de: Option<&str>) -> Appearance {
    if cfg!(target_os = "macos") {
        return crate::macos::appearance();
    }
    let read = |rel: &str| fs::read_to_string(home.join(rel)).ok();
    let mut a = Appearance::default();
    let de = de.unwrap_or("");

    if de.starts_with("KDE") {
        if let Some(t) = read(".config/kdeglobals") {
            a.theme = ini_value(&t, "KDE", "widgetStyle")
                .or_else(|| ini_value(&t, "General", "ColorScheme"));
            a.icons = ini_value(&t, "Icons", "Theme");
            a.font = ini_value(&t, "General", "font").map(|f| qt_font(&f));
        }
        if let Some(t) = read(".config/kcminputrc") {
            a.cursor = ini_value(&t, "Mouse", "cursorTheme")
                .map(|n| with_size(n, ini_value(&t, "Mouse", "cursorSize")));
        }
    }
    for rel in [
        ".config/gtk-3.0/settings.ini",
        ".config/gtk-4.0/settings.ini",
    ] {
        let Some(t) = read(rel) else { continue };
        let get = |k: &str| ini_value(&t, "Settings", k);
        a.theme = a.theme.take().or_else(|| get("gtk-theme-name"));
        a.icons = a.icons.take().or_else(|| get("gtk-icon-theme-name"));
        a.cursor = a.cursor.take().or_else(|| {
            get("gtk-cursor-theme-name").map(|n| with_size(n, get("gtk-cursor-theme-size")))
        });
        a.font = a
            .font
            .take()
            .or_else(|| get("gtk-font-name").map(|f| pango_font(&f)));
    }
    if de.starts_with("GNOME") && (a.theme.is_none() || a.icons.is_none() || a.font.is_none()) {
        a.theme = a.theme.take().or_else(|| gsettings("gtk-theme"));
        a.icons = a.icons.take().or_else(|| gsettings("icon-theme"));
        a.cursor = a
            .cursor
            .take()
            .or_else(|| gsettings("cursor-theme").map(|n| with_size(n, gsettings("cursor-size"))));
        a.font = a
            .font
            .take()
            .or_else(|| gsettings("font-name").map(|f| pango_font(&f)));
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn detects_common_desktops() {
        let de = |v: &'static str| detect_de(&env(&[("XDG_CURRENT_DESKTOP", v)]));
        assert_eq!(de("GNOME").as_deref(), Some("GNOME"));
        assert_eq!(de("ubuntu:GNOME").as_deref(), Some("GNOME"));
        assert_eq!(de("X-Cinnamon").as_deref(), Some("Cinnamon"));
        assert_eq!(de("XFCE").as_deref(), Some("Xfce"));
        assert_eq!(de("SomethingNew").as_deref(), Some("SomethingNew"));
        assert_eq!(detect_de(&env(&[])), None);
    }

    #[test]
    fn kde_includes_plasma_version() {
        let e = env(&[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "6")]);
        assert_eq!(detect_de(&e).as_deref(), Some("KDE Plasma 6"));
    }

    #[test]
    fn fallback_to_desktop_session() {
        assert_eq!(
            detect_de(&env(&[("DESKTOP_SESSION", "mate")])).as_deref(),
            Some("MATE")
        );
    }

    #[test]
    fn wm_from_environment_and_de() {
        let e = env(&[("SWAYSOCK", "/run/user/1000/sway.sock")]);
        assert_eq!(detect_wm(&e, None).as_deref(), Some("Sway"));
        assert_eq!(
            detect_wm(&env(&[]), Some("GNOME")).as_deref(),
            Some("Mutter")
        );
        assert_eq!(
            detect_wm(&env(&[]), Some("KDE Plasma 6")).as_deref(),
            Some("KWin")
        );
        assert_eq!(detect_wm(&env(&[("XDG_SESSION_TYPE", "tty")]), None), None);
    }

    #[test]
    fn wm_from_process_list() {
        assert_eq!(wm_from_processes(&["bash", "i3", "Xorg"]), Some("i3"));
        assert_eq!(wm_from_processes(&["bash", "sshd"]), None);
        // Compositors take priority over generic ones.
        assert_eq!(wm_from_processes(&["openbox", "sway"]), Some("Sway"));
    }

    #[test]
    fn wm_display_includes_session() {
        let d = DesktopInfo {
            de: None,
            wm: Some("Mutter".into()),
            session_type: Some("Wayland".into()),
        };
        assert_eq!(d.wm_display().as_deref(), Some("Mutter (Wayland)"));
    }

    #[test]
    fn connector_names() {
        assert_eq!(connector_name("card1-eDP-1"), Some("eDP-1"));
        assert_eq!(connector_name("card0-HDMI-A-1"), Some("HDMI-A-1"));
        assert_eq!(connector_name("card0"), None);
        assert_eq!(connector_name("renderD128"), None);
    }

    fn sample_edid() -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        // 1920x1080 @ 60 Hz detailed timing (148.5 MHz, 2200x1125 total)
        b[54..72].copy_from_slice(&[
            0x02, 0x3a, 0x80, 0x18, 0x71, 0x38, 0x2d, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        // monitor name descriptor
        let mut name = [0u8; 18];
        name[3] = 0xfc;
        name[5..9].copy_from_slice(b"DELL");
        name[9] = b'\n';
        name[10..].fill(b' ');
        b[72..90].copy_from_slice(&name);
        b
    }

    #[test]
    fn parses_edid_name_and_refresh() {
        let e = parse_edid(&sample_edid()).unwrap();
        assert_eq!(e.monitor.as_deref(), Some("DELL"));
        let (w, h, hz) = e.preferred.unwrap();
        assert_eq!((w, h), (1920, 1080));
        assert!((hz - 60.0).abs() < 0.01, "{hz}");
    }

    #[test]
    fn rejects_bad_edid() {
        assert!(parse_edid(&[0u8; 128]).is_none());
        assert!(parse_edid(&[0u8; 10]).is_none());
    }

    #[test]
    fn ini_and_font_parsing() {
        let ini = "# c\n[Settings]\ngtk-theme-name=MaterialYou\ngtk-font-name = Noto Sans Mono 11\n[Other]\ngtk-theme-name=No\n";
        assert_eq!(
            ini_value(ini, "Settings", "gtk-theme-name").as_deref(),
            Some("MaterialYou")
        );
        assert_eq!(
            ini_value(ini, "Other", "gtk-theme-name").as_deref(),
            Some("No")
        );
        assert_eq!(ini_value(ini, "Settings", "missing"), None);
        assert_eq!(pango_font("Noto Sans Mono 11"), "Noto Sans Mono (11pt)");
        assert_eq!(pango_font("Cantarell"), "Cantarell");
        assert_eq!(
            qt_font("Noto Sans,10,-1,5,50,0,0,0,0,0"),
            "Noto Sans (10pt)"
        );
    }

    #[test]
    fn appearance_from_gtk_and_kde_files() {
        let dir = std::env::temp_dir().join(format!("openfetch-appearance-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".config/gtk-3.0")).unwrap();
        std::fs::write(
            dir.join(".config/gtk-3.0/settings.ini"),
            "[Settings]\ngtk-theme-name=Adwaita\ngtk-icon-theme-name=Tela\ngtk-cursor-theme-name=Bibata\ngtk-cursor-theme-size=24\ngtk-font-name=Cantarell 11\n",
        )
        .unwrap();
        let a = detect_appearance(&dir, Some("SomethingElse"));
        assert_eq!(a.theme.as_deref(), Some("Adwaita"));
        assert_eq!(a.icons.as_deref(), Some("Tela"));
        assert_eq!(a.cursor.as_deref(), Some("Bibata (24px)"));
        assert_eq!(a.font.as_deref(), Some("Cantarell (11pt)"));
        std::fs::write(
            dir.join(".config/kdeglobals"),
            "[KDE]\nwidgetStyle=Breeze\n[Icons]\nTheme=breeze-dark\n",
        )
        .unwrap();
        let k = detect_appearance(&dir, Some("KDE Plasma 6"));
        assert_eq!(k.theme.as_deref(), Some("Breeze"));
        assert_eq!(k.icons.as_deref(), Some("breeze-dark"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn displays_does_not_panic() {
        let _ = displays();
    }
}
