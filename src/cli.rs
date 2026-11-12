//! Command-line interface.

use clap::Parser;

#[derive(Parser, Debug, PartialEq)]
#[command(
    name = "openfetch",
    version,
    about = "A fast, customisable system information and visualisation tool for Linux",
    after_help = "Configuration: ~/.config/openfetch/config.toml (see `openfetch --init-config`)"
)]
pub struct Cli {
    /// Use the compact layout (small logo, essential modules only)
    #[arg(short, long)]
    pub compact: bool,

    /// Print all detected information as JSON and exit
    #[arg(long)]
    pub json: bool,

    /// Print only the logo (optionally a specific one, e.g. `--logo arch`)
    #[arg(long, value_name = "NAME", num_args = 0..=1, default_missing_value = "")]
    pub logo: Option<String>,

    /// Animate the logo; optional effect: rainbow, wave, pulse, custom
    #[arg(long, value_name = "EFFECT", num_args = 0..=1, default_missing_value = "")]
    pub animation: Option<String>,

    /// With PATH: use that config file. Without: show the active configuration
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    pub config: Option<String>,

    /// Write a documented default config file to the default location
    #[arg(long)]
    pub init_config: bool,

    /// Show the logo of this distro instead of the detected one
    #[arg(long, value_name = "NAME")]
    pub distro: Option<String>,

    /// Use a PNG, JPEG or GIF as the logo
    #[arg(long, value_name = "PATH")]
    pub image: Option<String>,

    /// Live full-screen dashboard (usage graphs, optional spinning logo)
    #[arg(long)]
    pub tui: bool,

    /// Refresh interval for --tui in milliseconds
    #[arg(long, value_name = "MS")]
    pub interval: Option<u64>,

    /// Colour theme to use (overrides the config file)
    #[arg(long, value_name = "NAME")]
    pub theme: Option<String>,

    /// Use plain ASCII only (no Unicode bars or rules)
    #[arg(long)]
    pub ascii: bool,

    /// Disable colours
    #[arg(long)]
    pub no_color: bool,

    /// Hide IP addresses (for screenshots and recordings you share)
    #[arg(long = "noip", visible_alias = "no-ip")]
    pub noip: bool,

    /// List the built-in themes and exit
    #[arg(long)]
    pub list_themes: bool,

    /// List the built-in logos and exit
    #[arg(long)]
    pub list_logos: bool,
}

impl Cli {
    /// `--config` without a value asks to show the active configuration.
    pub fn show_config(&self) -> bool {
        matches!(self.config.as_deref(), Some(""))
    }

    /// `--config PATH` selects an alternative configuration file.
    pub fn config_path(&self) -> Option<&std::path::Path> {
        self.config
            .as_deref()
            .filter(|p| !p.is_empty())
            .map(std::path::Path::new)
    }

    /// `--logo [NAME]`: `Some(None)` means "auto-detected logo".
    pub fn logo_only(&self) -> Option<Option<&str>> {
        self.logo
            .as_deref()
            .map(|n| if n.is_empty() { None } else { Some(n) })
    }

    /// `--animation [EFFECT]`.
    pub fn animation_request(&self) -> Option<Option<&str>> {
        self.animation
            .as_deref()
            .map(|n| if n.is_empty() { None } else { Some(n) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        let mut v = vec!["openfetch"];
        v.extend_from_slice(args);
        Cli::try_parse_from(v).expect("valid arguments")
    }

    #[test]
    fn no_arguments() {
        let c = parse(&[]);
        assert!(!c.compact && !c.json);
        assert!(c.logo_only().is_none() && c.animation_request().is_none());
        assert!(!c.show_config() && c.config_path().is_none());
    }

    #[test]
    fn flags() {
        let c = parse(&["--compact", "--json", "--ascii", "--no-color"]);
        assert!(c.compact && c.json && c.ascii && c.no_color);
        assert!(parse(&["-c"]).compact);
    }

    #[test]
    fn optional_values() {
        assert_eq!(parse(&["--logo"]).logo_only(), Some(None));
        assert_eq!(parse(&["--logo", "arch"]).logo_only(), Some(Some("arch")));
        assert_eq!(parse(&["--animation"]).animation_request(), Some(None));
        assert_eq!(
            parse(&["--animation", "wave"]).animation_request(),
            Some(Some("wave"))
        );
        let c = parse(&["--config"]);
        assert!(c.show_config() && c.config_path().is_none());
        let c = parse(&["--config", "/tmp/x.toml"]);
        assert!(!c.show_config());
        assert_eq!(c.config_path(), Some(std::path::Path::new("/tmp/x.toml")));
    }

    #[test]
    fn tui_and_image_flags() {
        let c = parse(&["--tui", "--interval", "500", "--image", "/tmp/a.gif"]);
        assert!(c.tui);
        assert_eq!(c.interval, Some(500));
        assert_eq!(c.image.as_deref(), Some("/tmp/a.gif"));
        assert!(Cli::try_parse_from(["openfetch", "--interval", "fast"]).is_err());
    }

    #[test]
    fn noip_flag_and_alias() {
        assert!(parse(&["--noip"]).noip);
        assert!(parse(&["--no-ip"]).noip);
        assert!(!parse(&[]).noip);
    }

    #[test]
    fn rejects_unknown_flags() {
        assert!(Cli::try_parse_from(["openfetch", "--bogus"]).is_err());
    }
}
