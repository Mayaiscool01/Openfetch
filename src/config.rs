//! Configuration file (`~/.config/openfetch/config.toml`).

use crate::theme::{ColorSpec, ThemeDef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The documented default configuration written by `--init-config`.
pub const DEFAULT_CONFIG: &str = include_str!("default_config.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutStyle {
    Detailed,
    Compact,
}

/// `panels`: titled boxes from `[[sections]]`; `flat`: one list from `modules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frame {
    Panels,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogoPosition {
    Left,
    Right,
    None,
}

/// An entry of the `modules` list: a module name or a custom text row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModuleEntry {
    Name(String),
    Custom { label: String, text: String },
}

/// One titled box in the panel layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionConfig {
    pub title: String,
    pub modules: Vec<ModuleEntry>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bullet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub labels: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: String,
    pub unicode: Mode,
    pub color: Mode,
    pub cpu_usage: bool,
    /// Never show IP addresses (same as `--noip`).
    pub hide_ip: bool,
    pub hide_mounts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub style: LayoutStyle,
    pub frame: Frame,
    pub bullet: String,
    pub show_labels: bool,
    pub logo_position: LogoPosition,
    pub gap: usize,
    pub margin: usize,
    pub separator: String,
    pub rule: String,
    pub key_align: bool,
    pub bars: bool,
    pub bar_width: usize,
    pub compact_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogoConfig {
    pub name: String,
    /// `auto` (solid block art on Unicode terminals), `block` or `ascii`.
    pub style: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<Vec<ColorSpec>>,
    /// PNG / JPEG / GIF drawn with half-blocks instead of a text logo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Width of the image logo in terminal columns.
    pub image_width: usize,
}

/// Settings of the live full-screen mode (`openfetch --tui`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Milliseconds between data refreshes (250-10000).
    pub interval_ms: u64,
    /// Logo effect: spin, rainbow, wave, pulse, custom or none.
    pub effect: String,
    /// Samples kept for the CPU / network graphs.
    pub history: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub effect: String,
    pub fps: u32,
    pub speed: f32,
    pub duration: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // NOTE: plain values/arrays must come before tables for TOML serialisation.
    pub modules: Vec<ModuleEntry>,
    pub sections: Vec<SectionConfig>,
    pub general: General,
    pub layout: Layout,
    pub labels: BTreeMap<String, String>,
    pub logo: LogoConfig,
    pub animation: AnimationConfig,
    pub tui: TuiConfig,
    pub themes: BTreeMap<String, ThemeDef>,
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn entries(list: &[&str]) -> Vec<ModuleEntry> {
    list.iter()
        .map(|s| ModuleEntry::Name(s.to_string()))
        .collect()
}

/// The two panels of the default layout.
pub fn default_sections() -> Vec<SectionConfig> {
    vec![
        SectionConfig {
            title: "Software".into(),
            modules: entries(&[
                "os", "kernel", "uptime", "packages", "shell", "init", "break", "de", "wm",
                "theme", "icons", "cursor", "font", "terminal",
            ]),
            bullet: None,
            labels: None,
        },
        SectionConfig {
            title: "Hardware".into(),
            modules: entries(&[
                "host",
                "motherboard",
                "bios",
                "cpu",
                "cpu_usage",
                "cpu_temp",
                "gpu",
                "memory",
                "swap",
                "disk",
                "battery",
                "break",
                "resolution",
                "network",
                "localip",
                "load",
                "processes",
            ]),
            bullet: None,
            labels: None,
        },
    ]
}

impl Default for General {
    fn default() -> Self {
        General {
            theme: "default".into(),
            unicode: Mode::Auto,
            color: Mode::Auto,
            cpu_usage: true,
            hide_ip: false,
            hide_mounts: names(&["/boot/efi", "/efi"]),
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            style: LayoutStyle::Detailed,
            frame: Frame::Panels,
            bullet: "→".into(),
            show_labels: false,
            logo_position: LogoPosition::Left,
            gap: 3,
            margin: 1,
            separator: ":".into(),
            rule: "─".into(),
            key_align: true,
            bars: false,
            bar_width: 12,
            compact_modules: names(&[
                "title", "os", "kernel", "uptime", "shell", "de", "cpu", "memory", "disk",
                "localip", "battery",
            ]),
        }
    }
}

impl Default for LogoConfig {
    fn default() -> Self {
        LogoConfig {
            name: "auto".into(),
            style: "auto".into(),
            file: None,
            lines: None,
            colors: None,
            image: None,
            image_width: 36,
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        TuiConfig {
            interval_ms: 1000,
            effect: "spin".into(),
            history: 60,
        }
    }
}

impl Default for AnimationConfig {
    fn default() -> Self {
        AnimationConfig {
            enabled: false,
            effect: "auto".into(),
            fps: 15,
            speed: 1.0,
            duration: 0.0,
            file: None,
            frames: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            modules: names(&[
                "title",
                "rule",
                "os",
                "host",
                "kernel",
                "uptime",
                "packages",
                "shell",
                "terminal",
                "de",
                "wm",
                "resolution",
                "motherboard",
                "bios",
                "cpu",
                "cpu_usage",
                "cpu_temp",
                "gpu",
                "memory",
                "swap",
                "disk",
                "battery",
                "network",
                "localip",
                "break",
                "colors",
            ])
            .into_iter()
            .map(ModuleEntry::Name)
            .collect(),
            sections: default_sections(),
            general: General::default(),
            layout: Layout::default(),
            labels: BTreeMap::new(),
            logo: LogoConfig::default(),
            animation: AnimationConfig::default(),
            tui: TuiConfig::default(),
            themes: BTreeMap::new(),
        }
    }
}

impl Config {
    /// Module list for the selected layout style.
    pub fn active_modules(&self, compact: bool) -> Vec<ModuleEntry> {
        if compact {
            self.layout
                .compact_modules
                .iter()
                .cloned()
                .map(ModuleEntry::Name)
                .collect()
        } else {
            self.modules.clone()
        }
    }
}

/// Where the configuration lives: `$XDG_CONFIG_HOME` or `~/.config`.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("openfetch").join("config.toml")
}

/// Expand a leading `~/` using `$HOME`.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(path.trim_start_matches("~").trim_start_matches('/'));
        }
    }
    PathBuf::from(path)
}

pub struct Loaded {
    pub config: Config,
    pub path: PathBuf,
    pub found: bool,
}

/// Load configuration. A missing default file yields built-in defaults;
/// an explicitly requested file must exist and parse.
pub fn load(explicit: Option<&Path>) -> Result<Loaded, String> {
    let path = explicit.map(Path::to_path_buf).unwrap_or_else(default_path);
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(config) => Ok(Loaded {
                config,
                path,
                found: true,
            }),
            Err(e) if explicit.is_some() => Err(format!("invalid config {}: {e}", path.display())),
            Err(e) => {
                eprintln!("openfetch: ignoring invalid config {}: {e}", path.display());
                Ok(Loaded {
                    config: Config::default(),
                    path,
                    found: true,
                })
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => Ok(Loaded {
            config: Config::default(),
            path,
            found: false,
        }),
        Err(e) => Err(format!("cannot read config {}: {e}", path.display())),
    }
}

/// Write the documented default configuration (refuses to overwrite).
pub fn init(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, DEFAULT_CONFIG).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_default_matches_builtin_default() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG).expect("default_config.toml parses");
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn empty_file_gives_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn partial_sections_keep_other_defaults() {
        let c: Config = toml::from_str(
            r##"
            [layout]
            style = "compact"
            gap = 5
            [general]
            theme = "nord"
            "##,
        )
        .unwrap();
        assert_eq!(c.layout.style, LayoutStyle::Compact);
        assert_eq!(c.layout.gap, 5);
        assert_eq!(c.layout.margin, 1);
        assert_eq!(c.general.theme, "nord");
        assert!(c.general.cpu_usage);
    }

    #[test]
    fn modules_accept_names_and_custom_rows() {
        let c: Config =
            toml::from_str(r##"modules = ["os", { label = "Motto", text = "hi" }, "memory"]"##)
                .unwrap();
        assert_eq!(
            c.modules,
            vec![
                ModuleEntry::Name("os".into()),
                ModuleEntry::Custom {
                    label: "Motto".into(),
                    text: "hi".into()
                },
                ModuleEntry::Name("memory".into()),
            ]
        );
    }

    #[test]
    fn custom_themes_parse() {
        let c: Config = toml::from_str(
            r##"
            [themes.mine]
            base = "dracula"
            key = "#ff0000"
            logo = ["red", "#00ff00"]
            "##,
        )
        .unwrap();
        let t = &c.themes["mine"];
        assert_eq!(t.base.as_deref(), Some("dracula"));
        assert_eq!(t.key, Some(ColorSpec::Rgb(255, 0, 0)));
        assert_eq!(t.logo.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn invalid_values_are_errors() {
        assert!(toml::from_str::<Config>("[layout]\nstyle = \"huge\"").is_err());
        assert!(toml::from_str::<Config>("[general]\ntheme = 3").is_err());
    }

    #[test]
    fn effective_config_serialises() {
        let text = toml::to_string_pretty(&Config::default()).expect("serialises");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, Config::default());
    }

    #[test]
    fn tilde_expansion() {
        std::env::set_var("HOME", "/home/test");
        assert_eq!(expand_tilde("~/a/b"), PathBuf::from("/home/test/a/b"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
    }
}
