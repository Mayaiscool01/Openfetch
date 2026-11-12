//! Colours, colour-depth detection and themes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// A terminal colour as written in the configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ColorSpec {
    /// One of the 16 standard ANSI colours (0-15).
    Ansi(u8),
    /// An xterm-256 palette index (16-255).
    Indexed(u8),
    /// A 24-bit colour.
    Rgb(u8, u8, u8),
}

const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// Approximate RGB values for the 16 ANSI colours (used for blending/downgrading).
const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 49, 49),
    (13, 188, 121),
    (229, 229, 16),
    (36, 114, 200),
    (188, 63, 188),
    (17, 168, 205),
    (229, 229, 229),
    (102, 102, 102),
    (241, 76, 76),
    (35, 209, 139),
    (245, 245, 67),
    (59, 142, 234),
    (214, 112, 214),
    (41, 184, 219),
    (255, 255, 255),
];

fn parse_hex(hex: &str) -> Option<ColorSpec> {
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<Vec<_>>>()?;
    match digits.len() {
        3 => Some(ColorSpec::Rgb(
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17,
        )),
        6 => Some(ColorSpec::Rgb(
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        )),
        _ => None,
    }
}

impl FromStr for ColorSpec {
    type Err = String;

    /// Accepts `red`, `bright_blue`, `gray`, `#ff8800`, `#f80` or `0`-`255`.
    fn from_str(s: &str) -> Result<Self, String> {
        let t = s.trim().to_ascii_lowercase();
        if let Some(hex) = t.strip_prefix('#') {
            return parse_hex(hex).ok_or_else(|| format!("invalid hex colour '{s}'"));
        }
        if let Ok(n) = t.parse::<u8>() {
            return Ok(if n < 16 {
                ColorSpec::Ansi(n)
            } else {
                ColorSpec::Indexed(n)
            });
        }
        let norm = t.replace(['-', ' '], "_");
        if norm == "gray" || norm == "grey" {
            return Ok(ColorSpec::Ansi(8));
        }
        let (bright, base) = match norm.strip_prefix("bright") {
            Some(rest) => (true, rest.trim_start_matches('_')),
            None => (false, norm.as_str()),
        };
        NAMES
            .iter()
            .position(|n| *n == base)
            .map(|i| ColorSpec::Ansi(i as u8 + if bright { 8 } else { 0 }))
            .ok_or_else(|| format!("unknown colour '{s}'"))
    }
}

impl fmt::Display for ColorSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ColorSpec::Ansi(n) if n < 8 => write!(f, "{}", NAMES[n as usize]),
            ColorSpec::Ansi(n) => write!(f, "bright_{}", NAMES[(n as usize - 8) % 8]),
            ColorSpec::Indexed(n) => write!(f, "{n}"),
            ColorSpec::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl TryFrom<String> for ColorSpec {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<ColorSpec> for String {
    fn from(c: ColorSpec) -> String {
        c.to_string()
    }
}

/// How many colours the terminal can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorDepth {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

impl ColorDepth {
    pub fn detect() -> Self {
        Self::from_env(|k| std::env::var(k).ok())
    }

    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let colorterm = get("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorDepth::TrueColor;
        }
        let term = get("TERM").unwrap_or_default().to_ascii_lowercase();
        if term == "dumb" {
            return ColorDepth::None;
        }
        if ["kitty", "alacritty", "foot", "wezterm", "ghostty"]
            .iter()
            .any(|t| term.contains(t))
        {
            return ColorDepth::TrueColor;
        }
        if term.contains("256color") {
            return ColorDepth::Ansi256;
        }
        if term.is_empty() || term == "linux" || term == "vt100" || term == "xterm" {
            return ColorDepth::Ansi16;
        }
        ColorDepth::Ansi256
    }
}

fn dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

fn nearest16(rgb: (u8, u8, u8)) -> u8 {
    (0..16u8)
        .min_by_key(|i| dist(ANSI16[*i as usize], rgb))
        .unwrap_or(7)
}

fn rgb_to_256(rgb: (u8, u8, u8)) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |v: u8| -> usize {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            ((v as usize) - 35) / 40
        }
    };
    let (ri, gi, bi) = (level(rgb.0), level(rgb.1), level(rgb.2));
    let cube_rgb = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let cube_idx = 16 + 36 * ri + 6 * gi + bi;
    let avg = (rgb.0 as u32 + rgb.1 as u32 + rgb.2 as u32) / 3;
    let gray_i = if avg > 238 {
        23
    } else {
        (avg.saturating_sub(3) / 10) as usize
    };
    let gv = (8 + 10 * gray_i) as u8;
    if dist((gv, gv, gv), rgb) < dist(cube_rgb, rgb) {
        (232 + gray_i) as u8
    } else {
        cube_idx as u8
    }
}

impl ColorSpec {
    /// Approximate RGB value of this colour.
    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            ColorSpec::Rgb(r, g, b) => (r, g, b),
            ColorSpec::Ansi(n) => ANSI16[(n as usize) % 16],
            ColorSpec::Indexed(n) if n < 16 => ANSI16[n as usize],
            ColorSpec::Indexed(n) if n >= 232 => {
                let v = 8 + 10 * (n - 232);
                (v, v, v)
            }
            ColorSpec::Indexed(n) => {
                let i = (n - 16) as usize;
                let lv = |v: usize| if v == 0 { 0 } else { (55 + 40 * v) as u8 };
                (lv(i / 36), lv((i / 6) % 6), lv(i % 6))
            }
        }
    }

    /// Reduce the colour to something the terminal can show.
    pub fn downgrade(self, depth: ColorDepth) -> ColorSpec {
        match (self, depth) {
            (c, ColorDepth::TrueColor) => c,
            (ColorSpec::Rgb(r, g, b), ColorDepth::Ansi256) => {
                ColorSpec::Indexed(rgb_to_256((r, g, b)))
            }
            (c @ (ColorSpec::Ansi(_) | ColorSpec::Indexed(_)), ColorDepth::Ansi256) => c,
            (ColorSpec::Ansi(n), _) => ColorSpec::Ansi(n),
            (c, _) => ColorSpec::Ansi(nearest16(c.to_rgb())),
        }
    }

    /// Linear blend towards `other` (`t` = 0.0 keeps `self`, 1.0 gives `other`).
    pub fn mix(self, other: ColorSpec, t: f32) -> ColorSpec {
        let (a, b) = (self.to_rgb(), other.to_rgb());
        let t = t.clamp(0.0, 1.0);
        let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        ColorSpec::Rgb(f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
    }

    /// Scale brightness by `factor` (0.0 = black).
    pub fn scale(self, factor: f32) -> ColorSpec {
        let (r, g, b) = self.to_rgb();
        let f = |x: u8| (x as f32 * factor).clamp(0.0, 255.0).round() as u8;
        ColorSpec::Rgb(f(r), f(g), f(b))
    }
}

/// A fully resolved theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub title: ColorSpec,
    pub key: ColorSpec,
    pub value: ColorSpec,
    pub separator: ColorSpec,
    pub rule: ColorSpec,
    pub bar: ColorSpec,
    pub bar_empty: ColorSpec,
    pub warn: ColorSpec,
    pub critical: ColorSpec,
    /// Overrides the distro's logo palette when set.
    pub logo: Option<Vec<ColorSpec>>,
}

impl Default for Theme {
    fn default() -> Self {
        builtin("default").expect("default theme exists")
    }
}

/// A theme as written in `[themes.<name>]`; every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeDef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_empty: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<Vec<ColorSpec>>,
}

impl ThemeDef {
    fn apply(&self, mut t: Theme) -> Theme {
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(v) = self.$f { t.$f = v; } )* };
        }
        set!(title, key, value, separator, rule, bar, bar_empty, warn, critical);
        if let Some(l) = &self.logo {
            t.logo = Some(l.clone());
        }
        t
    }
}

pub const BUILTIN_NAMES: [&str; 7] = [
    "default", "ocean", "sunset", "forest", "nord", "dracula", "mono",
];

fn hex(s: &str) -> ColorSpec {
    s.parse().expect("valid built-in colour")
}

fn palette(list: &[&str]) -> Option<Vec<ColorSpec>> {
    Some(list.iter().map(|s| hex(s)).collect())
}

/// Look up one of the built-in themes.
pub fn builtin(name: &str) -> Option<Theme> {
    let t = |title: &str,
             key: &str,
             value: &str,
             sep: &str,
             bar: &str,
             empty: &str,
             warn: &str,
             crit: &str,
             logo: &[&str]| Theme {
        title: hex(title),
        key: hex(key),
        value: hex(value),
        separator: hex(sep),
        rule: hex(sep),
        bar: hex(bar),
        bar_empty: hex(empty),
        warn: hex(warn),
        critical: hex(crit),
        logo: palette(logo),
    };
    Some(match name {
        "default" => Theme {
            title: ColorSpec::Ansi(14),
            key: ColorSpec::Ansi(12),
            value: ColorSpec::Ansi(7),
            separator: ColorSpec::Ansi(8),
            rule: ColorSpec::Ansi(8),
            bar: ColorSpec::Ansi(10),
            bar_empty: ColorSpec::Ansi(8),
            warn: ColorSpec::Ansi(11),
            critical: ColorSpec::Ansi(9),
            logo: None,
        },
        "ocean" => t(
            "#7fdbff",
            "#39a0ed",
            "#d6e4f0",
            "#4a6b8a",
            "#2ec4b6",
            "#2b3a4a",
            "#ffd166",
            "#ef476f",
            &["#39a0ed", "#7fdbff", "#d6e4f0"],
        ),
        "sunset" => t(
            "#ff9e64",
            "#f7768e",
            "#f5e0dc",
            "#7a5c61",
            "#ffb86c",
            "#3b2f35",
            "#f1fa8c",
            "#ff5555",
            &["#ff5f6d", "#ffc371", "#fff0c9"],
        ),
        "forest" => t(
            "#a3be8c",
            "#7ec699",
            "#d8e2dc",
            "#587464",
            "#a3be8c",
            "#2f3d35",
            "#ebcb8b",
            "#bf616a",
            &["#7ec699", "#a3be8c", "#ebcb8b"],
        ),
        "nord" => t(
            "#88c0d0",
            "#81a1c1",
            "#e5e9f0",
            "#4c566a",
            "#a3be8c",
            "#3b4252",
            "#ebcb8b",
            "#bf616a",
            &["#88c0d0", "#81a1c1", "#5e81ac"],
        ),
        "dracula" => t(
            "#bd93f9",
            "#ff79c6",
            "#f8f8f2",
            "#6272a4",
            "#50fa7b",
            "#44475a",
            "#f1fa8c",
            "#ff5555",
            &["#bd93f9", "#ff79c6", "#8be9fd"],
        ),
        "mono" => Theme {
            title: ColorSpec::Ansi(15),
            key: ColorSpec::Ansi(15),
            value: ColorSpec::Ansi(7),
            separator: ColorSpec::Ansi(8),
            rule: ColorSpec::Ansi(8),
            bar: ColorSpec::Ansi(15),
            bar_empty: ColorSpec::Ansi(8),
            warn: ColorSpec::Ansi(15),
            critical: ColorSpec::Ansi(15),
            logo: Some(vec![
                ColorSpec::Ansi(15),
                ColorSpec::Ansi(7),
                ColorSpec::Ansi(8),
            ]),
        },
        _ => return None,
    })
}

/// Resolve a theme by name, looking at user-defined themes first.
pub fn resolve(name: &str, custom: &BTreeMap<String, ThemeDef>) -> Result<Theme, String> {
    resolve_depth(name, custom, 0)
}

fn resolve_depth(
    name: &str,
    custom: &BTreeMap<String, ThemeDef>,
    depth: usize,
) -> Result<Theme, String> {
    if depth > 8 {
        return Err(format!("theme '{name}': 'base' chain is too deep"));
    }
    if let Some(def) = custom.get(name) {
        let base = match &def.base {
            Some(b) if b != name => resolve_depth(b, custom, depth + 1)?,
            _ => builtin(name).unwrap_or_default(),
        };
        return Ok(def.apply(base));
    }
    builtin(name).ok_or_else(|| format!("unknown theme '{name}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_hex_and_numbers() {
        assert_eq!("red".parse(), Ok(ColorSpec::Ansi(1)));
        assert_eq!("Bright_Blue".parse(), Ok(ColorSpec::Ansi(12)));
        assert_eq!("brightcyan".parse(), Ok(ColorSpec::Ansi(14)));
        assert_eq!("gray".parse(), Ok(ColorSpec::Ansi(8)));
        assert_eq!("#ff8800".parse(), Ok(ColorSpec::Rgb(255, 136, 0)));
        assert_eq!("#f80".parse(), Ok(ColorSpec::Rgb(255, 136, 0)));
        assert_eq!("208".parse(), Ok(ColorSpec::Indexed(208)));
        assert_eq!("3".parse(), Ok(ColorSpec::Ansi(3)));
        assert!("#12".parse::<ColorSpec>().is_err());
        assert!("chartreuse".parse::<ColorSpec>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for s in ["red", "bright_magenta", "#0a1b2c", "208"] {
            let c: ColorSpec = s.parse().unwrap();
            assert_eq!(c.to_string().parse::<ColorSpec>().unwrap(), c);
        }
    }

    #[test]
    fn downgrades_truecolor() {
        let red = ColorSpec::Rgb(255, 0, 0);
        assert_eq!(red.downgrade(ColorDepth::TrueColor), red);
        assert_eq!(red.downgrade(ColorDepth::Ansi256), ColorSpec::Indexed(196));
        assert_eq!(red.downgrade(ColorDepth::Ansi16), ColorSpec::Ansi(1));
        assert_eq!(
            ColorSpec::Rgb(128, 128, 128).downgrade(ColorDepth::Ansi256),
            ColorSpec::Indexed(244)
        );
    }

    #[test]
    fn mix_and_scale() {
        let black = ColorSpec::Rgb(0, 0, 0);
        let white = ColorSpec::Rgb(255, 255, 255);
        assert_eq!(black.mix(white, 1.0), white);
        assert_eq!(black.mix(white, 0.0), black);
        assert_eq!(white.scale(0.0), black);
    }

    #[test]
    fn detects_depth() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            ColorDepth::from_env(env(&[("COLORTERM", "truecolor")])),
            ColorDepth::TrueColor
        );
        assert_eq!(
            ColorDepth::from_env(env(&[("TERM", "xterm-256color")])),
            ColorDepth::Ansi256
        );
        assert_eq!(
            ColorDepth::from_env(env(&[("TERM", "linux")])),
            ColorDepth::Ansi16
        );
        assert_eq!(
            ColorDepth::from_env(env(&[("TERM", "dumb")])),
            ColorDepth::None
        );
    }

    #[test]
    fn all_builtin_themes_exist() {
        for n in BUILTIN_NAMES {
            assert!(builtin(n).is_some(), "{n}");
        }
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn custom_theme_inherits_from_base() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "mine".to_string(),
            ThemeDef {
                base: Some("nord".into()),
                key: Some(ColorSpec::Rgb(1, 2, 3)),
                ..Default::default()
            },
        );
        let t = resolve("mine", &custom).unwrap();
        let nord = builtin("nord").unwrap();
        assert_eq!(t.key, ColorSpec::Rgb(1, 2, 3));
        assert_eq!(t.title, nord.title);
        assert!(resolve("missing", &custom).is_err());
    }

    #[test]
    fn base_cycles_are_rejected() {
        let mut custom = BTreeMap::new();
        for (a, b) in [("a", "b"), ("b", "a")] {
            custom.insert(
                a.to_string(),
                ThemeDef {
                    base: Some(b.into()),
                    ..Default::default()
                },
            );
        }
        assert!(resolve("a", &custom).is_err());
    }
}
