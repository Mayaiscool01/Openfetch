//! macOS probes.
//!
//! The parsers here are pure functions and are unit-tested on every platform.
//! The functions that run system tools (`sw_vers`, `system_profiler`, `pmset`,
//! `ioreg`, `route`, `defaults`, `ps`) are only called when running on macOS.

use crate::desktop::{Appearance, Display};
use crate::gpu::GpuInfo;
use crate::os::{OsRelease, PackageCount};
use crate::system::{BatteryInfo, Firmware};
use serde_json::Value;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Standard output of a command, if it ran successfully.
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ------------------------------------------------------------------- OS

/// Marketing name of a macOS version (`15.1` -> `Sequoia`).
pub fn codename(version: &str) -> Option<&'static str> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    Some(match (major, minor) {
        (26, _) => "Tahoe",
        (15, _) => "Sequoia",
        (14, _) => "Sonoma",
        (13, _) => "Ventura",
        (12, _) => "Monterey",
        (11, _) => "Big Sur",
        (10, 15) => "Catalina",
        (10, 14) => "Mojave",
        (10, 13) => "High Sierra",
        (10, 12) => "Sierra",
        _ => return None,
    })
}

pub fn os_release_from(product: &str, version: &str) -> OsRelease {
    let name = if product.trim().is_empty() {
        "macOS".to_string()
    } else {
        product.trim().to_string()
    };
    let version = version.trim();
    let pretty_name = match (version.is_empty(), codename(version)) {
        (true, _) => name.clone(),
        (false, Some(code)) => format!("{name} {version} {code}"),
        (false, None) => format!("{name} {version}"),
    };
    OsRelease {
        name,
        pretty_name,
        version: (!version.is_empty()).then(|| version.to_string()),
        id: "macos".to_string(),
        id_like: vec!["darwin".to_string()],
        arch: std::env::consts::ARCH.to_string(),
    }
}

pub fn os_release() -> OsRelease {
    let product = run("sw_vers", &["-productName"]).unwrap_or_default();
    let version = run("sw_vers", &["-productVersion"]).unwrap_or_default();
    os_release_from(&product, &version)
}

pub fn process_count() -> Option<usize> {
    let n = run("ps", &["-A", "-o", "pid="])?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    (n > 0).then_some(n)
}

fn count_dirs(path: &str) -> usize {
    std::fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

/// Homebrew formulae and casks (Apple Silicon and Intel prefixes), MacPorts.
pub fn packages() -> Vec<PackageCount> {
    let sum = |paths: &[&str]| paths.iter().map(|p| count_dirs(p)).sum::<usize>();
    [
        ("brew", sum(&["/opt/homebrew/Cellar", "/usr/local/Cellar"])),
        (
            "brew-cask",
            sum(&["/opt/homebrew/Caskroom", "/usr/local/Caskroom"]),
        ),
        ("macports", count_dirs("/opt/local/var/macports/software")),
    ]
    .into_iter()
    .filter(|(_, n)| *n > 0)
    .map(|(manager, count)| PackageCount {
        manager: manager.to_string(),
        count,
    })
    .collect()
}

// -------------------------------------------------------- system_profiler

/// Hardware and display data, fetched once (`system_profiler` is slow).
fn profiler() -> Option<&'static Value> {
    static CACHE: OnceLock<Option<Value>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let text = run(
                "system_profiler",
                &["-json", "SPHardwareDataType", "SPDisplaysDataType"],
            )?;
            serde_json::from_str(&text).ok()
        })
        .as_ref()
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn hardware(json: &Value) -> Option<&Value> {
    json.get("SPHardwareDataType")?.as_array()?.first()
}

/// `MacBook Air (MacBookAir10,1)`.
pub fn host_from(json: &Value) -> Option<String> {
    let hw = hardware(json)?;
    match (text(hw, "machine_name"), text(hw, "machine_model")) {
        (Some(name), Some(model)) => Some(format!("{name} ({model})")),
        (Some(one), None) | (None, Some(one)) => Some(one.to_string()),
        (None, None) => None,
    }
}

pub fn firmware_from(json: &Value) -> Option<Firmware> {
    let hw = hardware(json)?;
    let version = text(hw, "boot_rom_version")?.to_string();
    // Apple Silicon reports a chip type and boots with iBoot; Intel Macs use EFI.
    let mode = if text(hw, "chip_type").is_some() {
        "iBoot"
    } else {
        "EFI"
    };
    Some(Firmware {
        vendor: Some("Apple".to_string()),
        version: Some(version),
        date: None,
        mode: mode.to_string(),
        secure_boot: None,
    })
}

fn display_cards(json: &Value) -> Vec<&Value> {
    json.get("SPDisplaysDataType")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

pub fn gpus_from(json: &Value) -> Vec<GpuInfo> {
    let mut out: Vec<GpuInfo> = Vec::new();
    for card in display_cards(json) {
        let Some(name) = text(card, "sppci_model").or_else(|| text(card, "_name")) else {
            continue;
        };
        let name = if name.ends_with("GPU") || !name.starts_with("Apple M") {
            name.to_string()
        } else {
            format!("{name} GPU")
        };
        if !out.iter().any(|g| g.name == name) {
            out.push(GpuInfo { name, driver: None });
        }
    }
    out
}

/// `2560 x 1600` -> `2560x1600`.
fn compact_resolution(s: &str) -> Option<String> {
    let mut nums = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty());
    let (w, h) = (nums.next()?, nums.next()?);
    Some(format!("{w}x{h}"))
}

/// `1440 x 900 @ 60.00Hz` -> `60.0`.
fn refresh_rate(s: &str) -> Option<f32> {
    let after = s.split('@').nth(1)?;
    let number: String = after
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    number.parse().ok().filter(|hz: &f32| *hz > 0.0)
}

pub fn displays_from(json: &Value) -> Vec<Display> {
    let mut out = Vec::new();
    for card in display_cards(json) {
        let Some(list) = card.get("spdisplays_ndrvs").and_then(Value::as_array) else {
            continue;
        };
        for d in list {
            let scaled = text(d, "_spdisplays_resolution");
            let Some(resolution) = text(d, "_spdisplays_pixels")
                .or(scaled)
                .and_then(compact_resolution)
            else {
                continue;
            };
            out.push(Display {
                name: text(d, "_name").unwrap_or("Display").to_string(),
                monitor: None,
                resolution,
                refresh_hz: scaled.and_then(refresh_rate),
            });
        }
    }
    out
}

pub fn host_model() -> Option<String> {
    profiler()
        .and_then(host_from)
        .or_else(|| run("sysctl", &["-n", "hw.model"]).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
}

pub fn firmware() -> Option<Firmware> {
    profiler().and_then(firmware_from)
}

pub fn gpus() -> Vec<GpuInfo> {
    profiler().map(gpus_from).unwrap_or_default()
}

pub fn displays() -> Vec<Display> {
    profiler().map(displays_from).unwrap_or_default()
}

// ---------------------------------------------------------------- battery

/// Charge, state and minutes remaining from `pmset -g batt`.
pub fn parse_pmset(text: &str) -> Option<(u8, String, Option<u32>)> {
    let line = text.lines().find(|l| l.contains("InternalBattery"))?;
    let (before, after) = line.split_once('%')?;
    let digits: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let percent = digits.parse::<u8>().ok()?.min(100);
    let lower = after.to_ascii_lowercase();
    let status = if lower.contains("discharging") {
        "Discharging"
    } else if lower.contains("not charging") {
        "Not charging"
    } else if lower.contains("charged") {
        "Full"
    } else if lower.contains("charging") || lower.contains("finishing charge") {
        "Charging"
    } else {
        "Unknown"
    };
    let minutes = after.split(';').find_map(|part| {
        let part = part.trim();
        let time = part.strip_suffix("present: true").unwrap_or(part).trim();
        let (h, m) = time.strip_suffix("remaining")?.trim().split_once(':')?;
        let total = h.trim().parse::<u32>().ok()? * 60 + m.trim().parse::<u32>().ok()?;
        (total > 0).then_some(total)
    });
    Some((percent, status.to_string(), minutes))
}

/// Value of `"Key" = 123` in `ioreg` output.
pub fn ioreg_number(text: &str, key: &str) -> Option<f64> {
    let needle = format!("\"{key}\" = ");
    text.lines().find_map(|l| {
        let rest = l.trim().strip_prefix(&needle)?;
        rest.trim().parse().ok()
    })
}

pub fn batteries() -> Vec<BatteryInfo> {
    let Some((percent, status, minutes)) =
        run("pmset", &["-g", "batt"]).and_then(|t| parse_pmset(&t))
    else {
        return Vec::new();
    };
    let ioreg = run("ioreg", &["-rn", "AppleSmartBattery"]).unwrap_or_default();
    let design = ioreg_number(&ioreg, "DesignCapacity");
    let full = ioreg_number(&ioreg, "AppleRawMaxCapacity");
    let health = match (full, design) {
        (Some(f), Some(d)) if d > 0.0 && f > 0.0 => Some(((f / d * 100.0) as f32).min(100.0)),
        _ => None,
    };
    vec![BatteryInfo {
        name: "InternalBattery".to_string(),
        percent: Some(percent),
        status,
        health_percent: health,
        cycles: ioreg_number(&ioreg, "CycleCount")
            .map(|c| c as u32)
            .filter(|c| *c > 0),
        minutes_remaining: minutes,
    }]
}

// ------------------------------------------------------ network / desktop

/// Interface of the default route from `route -n get default`.
pub fn parse_default_interface(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let name = l.trim().strip_prefix("interface:")?.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

pub fn default_interface() -> Option<String> {
    run("route", &["-n", "get", "default"]).and_then(|t| parse_default_interface(&t))
}

/// Light or Dark appearance. `defaults` fails when the key is unset (Light).
pub fn appearance() -> Appearance {
    let dark = run("defaults", &["read", "-g", "AppleInterfaceStyle"])
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("dark"));
    Appearance {
        theme: Some(if dark { "Dark" } else { "Light" }.to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        serde_json::json!({
            "SPHardwareDataType": [{
                "machine_name": "MacBook Air",
                "machine_model": "MacBookAir10,1",
                "chip_type": "Apple M1",
                "boot_rom_version": "11881.41.5"
            }],
            "SPDisplaysDataType": [{
                "_name": "Apple M1",
                "sppci_model": "Apple M1",
                "spdisplays_ndrvs": [
                    {
                        "_name": "Color LCD",
                        "_spdisplays_pixels": "2560 x 1600",
                        "_spdisplays_resolution": "1440 x 900 @ 60.00Hz"
                    },
                    { "_name": "DELL U2720Q", "_spdisplays_resolution": "3840 x 2160 @ 59.94Hz" },
                    { "_name": "broken" }
                ]
            }]
        })
    }

    #[test]
    fn version_names() {
        assert_eq!(codename("15.1"), Some("Sequoia"));
        assert_eq!(codename("14"), Some("Sonoma"));
        assert_eq!(codename("26.0.1"), Some("Tahoe"));
        assert_eq!(codename("10.15.7"), Some("Catalina"));
        assert_eq!(codename("9.0"), None);
        assert_eq!(codename("garbage"), None);
    }

    #[test]
    fn builds_os_release() {
        let os = os_release_from("macOS\n", "15.1\n");
        assert_eq!(os.pretty_name, "macOS 15.1 Sequoia");
        assert_eq!(os.id, "macos");
        assert_eq!(os.version.as_deref(), Some("15.1"));
        let unknown = os_release_from("", "");
        assert_eq!(unknown.pretty_name, "macOS");
        assert_eq!(unknown.version, None);
        assert_eq!(os_release_from("macOS", "99.0").pretty_name, "macOS 99.0");
    }

    #[test]
    fn reads_hardware_from_system_profiler() {
        let json = sample();
        assert_eq!(
            host_from(&json).as_deref(),
            Some("MacBook Air (MacBookAir10,1)")
        );
        let fw = firmware_from(&json).unwrap();
        assert_eq!(fw.vendor.as_deref(), Some("Apple"));
        assert_eq!(fw.version.as_deref(), Some("11881.41.5"));
        assert_eq!(fw.mode, "iBoot");
        let gpus = gpus_from(&json);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "Apple M1 GPU");
    }

    #[test]
    fn intel_macs_and_missing_data() {
        let intel = serde_json::json!({
            "SPHardwareDataType": [{ "machine_model": "MacBookPro16,1", "boot_rom_version": "2022.100.22" }],
            "SPDisplaysDataType": [
                { "sppci_model": "Intel UHD Graphics 630" },
                { "sppci_model": "AMD Radeon Pro 5500M" }
            ]
        });
        assert_eq!(host_from(&intel).as_deref(), Some("MacBookPro16,1"));
        assert_eq!(firmware_from(&intel).unwrap().mode, "EFI");
        let names: Vec<String> = gpus_from(&intel).into_iter().map(|g| g.name).collect();
        assert_eq!(
            names,
            vec!["Intel UHD Graphics 630", "AMD Radeon Pro 5500M"]
        );
        let empty = serde_json::json!({});
        assert!(host_from(&empty).is_none() && firmware_from(&empty).is_none());
        assert!(gpus_from(&empty).is_empty() && displays_from(&empty).is_empty());
    }

    #[test]
    fn reads_displays() {
        let d = displays_from(&sample());
        assert_eq!(d.len(), 2, "entries without a resolution are skipped");
        assert_eq!(d[0].name, "Color LCD");
        assert_eq!(d[0].resolution, "2560x1600");
        assert_eq!(d[0].refresh_hz, Some(60.0));
        assert_eq!(d[1].resolution, "3840x2160");
        assert!((d[1].refresh_hz.unwrap() - 59.94).abs() < 0.001);
    }

    #[test]
    fn parses_pmset_output() {
        let discharging = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=4653155)\t85%; discharging; 4:32 remaining present: true\n";
        assert_eq!(
            parse_pmset(discharging),
            Some((85, "Discharging".to_string(), Some(272)))
        );
        let charging = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t47%; charging; 1:05 remaining present: true\n";
        assert_eq!(
            parse_pmset(charging),
            Some((47, "Charging".to_string(), Some(65)))
        );
        let full = " -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true\n";
        assert_eq!(parse_pmset(full), Some((100, "Full".to_string(), None)));
        let idle = " -InternalBattery-0 (id=1)\t80%; AC attached; not charging present: true\n";
        assert_eq!(
            parse_pmset(idle),
            Some((80, "Not charging".to_string(), None))
        );
        let estimating =
            " -InternalBattery-0 (id=1)\t60%; discharging; (no estimate) present: true\n";
        assert_eq!(
            parse_pmset(estimating),
            Some((60, "Discharging".to_string(), None))
        );
        assert_eq!(
            parse_pmset("Now drawing from 'AC Power'\n"),
            None,
            "desktop Macs"
        );
    }

    #[test]
    fn parses_ioreg_numbers() {
        let text = "+-o AppleSmartBattery  <class AppleSmartBattery>\n    {\n      \"CycleCount\" = 123\n      \"DesignCapacity\" = 4563\n      \"AppleRawMaxCapacity\" = 4107\n      \"Serial\" = \"ABC\"\n    }\n";
        assert_eq!(ioreg_number(text, "CycleCount"), Some(123.0));
        assert_eq!(ioreg_number(text, "DesignCapacity"), Some(4563.0));
        assert_eq!(ioreg_number(text, "Serial"), None);
        assert_eq!(ioreg_number(text, "Missing"), None);
    }

    #[test]
    fn parses_default_route() {
        let text = "   route to: default\ndestination: default\n    gateway: 192.168.1.1\n  interface: en0\n      flags: <UP,GATEWAY,DONE,STATIC>\n";
        assert_eq!(parse_default_interface(text).as_deref(), Some("en0"));
        assert_eq!(
            parse_default_interface("route: writing to routing socket"),
            None
        );
    }
}
