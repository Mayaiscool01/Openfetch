//! CPU model, topology, frequency, usage and temperature.

use serde::Serialize;
use std::fs;

#[derive(Debug, Clone, Serialize)]
pub struct CpuInfo {
    pub model: String,
    pub cores: Option<usize>,
    pub threads: usize,
    pub max_mhz: Option<u32>,
    pub usage_percent: Option<f32>,
    pub temperature_c: Option<f32>,
}

fn field<'a>(cpuinfo: &'a str, key: &str) -> Option<&'a str> {
    cpuinfo.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

/// ARM CPU part numbers -> marketing / core names.
fn arm_part_name(implementer: &str, part: &str) -> Option<&'static str> {
    let part = part.trim().trim_start_matches("0x").to_ascii_lowercase();
    match implementer.trim().trim_start_matches("0x") {
        // Apple Silicon (Asahi Linux reports the individual cores)
        "61" => match part.trim_start_matches('0') {
            "22" | "23" => Some("Apple M1"),
            "24" | "25" => Some("Apple M1 Pro"),
            "28" | "29" => Some("Apple M1 Max"),
            "30" | "31" => Some("Apple M2"),
            _ => Some("Apple Silicon"),
        },
        // Arm Ltd
        "41" => match part.as_str() {
            "d03" => Some("Cortex-A53"),
            "d04" => Some("Cortex-A35"),
            "d05" => Some("Cortex-A55"),
            "d07" => Some("Cortex-A57"),
            "d08" => Some("Cortex-A72"),
            "d09" => Some("Cortex-A73"),
            "d0a" => Some("Cortex-A75"),
            "d0b" => Some("Cortex-A76"),
            "d0c" => Some("Neoverse-N1"),
            "d0d" => Some("Cortex-A77"),
            "d40" => Some("Neoverse-V1"),
            "d41" => Some("Cortex-A78"),
            "d44" => Some("Cortex-X1"),
            "d46" => Some("Cortex-A510"),
            "d47" => Some("Cortex-A710"),
            "d48" => Some("Cortex-X2"),
            _ => None,
        },
        _ => None,
    }
}

fn tidy_model(s: &str) -> String {
    let mut out = s.to_string();
    for junk in ["(R)", "(TM)", "(tm)", "CPU ", " Processor", "  "] {
        out = out.replace(junk, "");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Work out the CPU model name from `/proc/cpuinfo` text.
pub fn model_from_cpuinfo(cpuinfo: &str) -> Option<String> {
    if let Some(m) = field(cpuinfo, "model name").filter(|m| !m.is_empty()) {
        // Some ARM kernels print generic strings like "ARMv8 Processor rev 0".
        if !m.to_ascii_lowercase().starts_with("armv") {
            return Some(tidy_model(m));
        }
    }
    if let (Some(imp), Some(part)) = (
        field(cpuinfo, "CPU implementer"),
        field(cpuinfo, "CPU part"),
    ) {
        if let Some(name) = arm_part_name(imp, part) {
            return Some(name.to_string());
        }
    }
    field(cpuinfo, "Hardware")
        .or_else(|| field(cpuinfo, "cpu model"))
        .or_else(|| field(cpuinfo, "Processor"))
        .map(tidy_model)
}

/// Lower rank = better candidate for "the CPU temperature".
pub fn rank_sensor(chip: &str, label: &str) -> Option<u8> {
    let l = label.to_ascii_lowercase();
    match chip {
        "coretemp" => Some(if l.starts_with("package") {
            0
        } else if l.starts_with("core") {
            2
        } else {
            3
        }),
        "k10temp" | "zenpower" => Some(if l == "tctl" || l == "tdie" { 0 } else { 2 }),
        "cpu_thermal" | "cpu-thermal" | "soc_thermal" => Some(1),
        c if c.contains("cpu") => Some(1),
        _ if l.contains("cpu") => Some(1),
        "acpitz" => Some(6),
        _ => None,
    }
}

fn read_trim(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Best-effort CPU temperature in °C from hwmon / thermal zones.
pub fn temperature() -> Option<f32> {
    let mut found: Vec<(u8, f32)> = Vec::new();
    fn consider(found: &mut Vec<(u8, f32)>, rank: u8, milli: f32) {
        let c = milli / 1000.0;
        if c > 0.0 && c < 150.0 {
            found.push((rank, c));
        }
    }

    if let Ok(dirs) = fs::read_dir("/sys/class/hwmon") {
        for dir in dirs.filter_map(Result::ok) {
            let path = dir.path();
            let chip = read_trim(&path.join("name")).unwrap_or_default();
            let Ok(files) = fs::read_dir(&path) else {
                continue;
            };
            for f in files.filter_map(Result::ok) {
                let name = f.file_name().to_string_lossy().into_owned();
                let Some(stem) = name
                    .strip_prefix("temp")
                    .and_then(|r| r.strip_suffix("_input"))
                else {
                    continue;
                };
                let label = read_trim(&path.join(format!("temp{stem}_label"))).unwrap_or_default();
                let Some(rank) = rank_sensor(&chip, &label) else {
                    continue;
                };
                if let Some(v) = read_trim(&f.path()).and_then(|s| s.parse::<f32>().ok()) {
                    consider(&mut found, rank, v);
                }
            }
        }
    }
    if found.is_empty() {
        if let Ok(zones) = fs::read_dir("/sys/class/thermal") {
            for z in zones.filter_map(Result::ok) {
                let p = z.path();
                let ty = read_trim(&p.join("type"))
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if ty.contains("cpu") || ty.contains("pkg") || ty.contains("soc") {
                    if let Some(v) = read_trim(&p.join("temp")).and_then(|s| s.parse::<f32>().ok())
                    {
                        consider(&mut found, 4, v);
                    }
                }
            }
        }
    }
    found
        .into_iter()
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1).reverse()))
        .map(|(_, c)| c)
}

fn max_mhz_from_sysfs() -> Option<u32> {
    let khz: u32 = read_trim(std::path::Path::new(
        "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq",
    ))?
    .parse()
    .ok()?;
    Some(khz / 1000)
}

/// Format core/thread counts: `8` when equal, `4C/8T` otherwise.
pub fn topology(cores: Option<usize>, threads: usize) -> String {
    match cores {
        Some(c) if c != threads => format!("{c}C/{threads}T"),
        _ => threads.to_string(),
    }
}

pub fn detect(sys: &sysinfo::System, usage: Option<f32>, want_temperature: bool) -> CpuInfo {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|b| !b.is_empty() && !b.to_ascii_lowercase().starts_with("armv"));
    let model = model_from_cpuinfo(&cpuinfo)
        .or(brand.map(|b| tidy_model(&b)))
        .unwrap_or_else(|| format!("Unknown ({})", std::env::consts::ARCH));
    let threads = sys.cpus().len().max(1);
    let max_mhz = max_mhz_from_sysfs().or_else(|| {
        let f = sys
            .cpus()
            .first()
            .map(|c| c.frequency() as u32)
            .unwrap_or(0);
        (f > 0).then_some(f)
    });
    CpuInfo {
        model,
        cores: sys.physical_core_count(),
        threads,
        max_mhz,
        usage_percent: usage,
        temperature_c: if want_temperature {
            temperature()
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_model() {
        let text = "processor\t: 0\nmodel name\t: AMD Ryzen(TM) 7 5800X 8-Core Processor\ncpu MHz\t\t: 3800.0\n";
        assert_eq!(
            model_from_cpuinfo(text).as_deref(),
            Some("AMD Ryzen 7 5800X 8-Core")
        );
        let intel = "model name\t: Intel(R) Core(TM) i7-1165G7 @ 2.80GHz\n";
        assert_eq!(
            model_from_cpuinfo(intel).as_deref(),
            Some("Intel Core i7-1165G7 @ 2.80GHz")
        );
    }

    #[test]
    fn apple_silicon_from_part_numbers() {
        let text = "processor : 0\nBogoMIPS : 48.00\nCPU implementer : 0x61\nCPU architecture: 8\nCPU variant : 0x1\nCPU part : 0x022\n\nprocessor : 4\nCPU implementer : 0x61\nCPU part : 0x023\n";
        assert_eq!(model_from_cpuinfo(text).as_deref(), Some("Apple M1"));
    }

    #[test]
    fn generic_arm_strings_fall_back_to_part_table() {
        let text =
            "model name : ARMv8 Processor rev 3 (v8l)\nCPU implementer : 0x41\nCPU part : 0xd08\n";
        assert_eq!(model_from_cpuinfo(text).as_deref(), Some("Cortex-A72"));
    }

    #[test]
    fn unknown_cpuinfo_gives_none() {
        assert_eq!(model_from_cpuinfo(""), None);
    }

    #[test]
    fn sensor_ranking() {
        assert_eq!(rank_sensor("coretemp", "Package id 0"), Some(0));
        assert!(rank_sensor("coretemp", "Core 1") > rank_sensor("coretemp", "Package id 0"));
        assert_eq!(rank_sensor("k10temp", "Tctl"), Some(0));
        assert_eq!(rank_sensor("nvme", "Composite"), None);
        assert!(rank_sensor("acpitz", "") > rank_sensor("cpu_thermal", ""));
    }

    #[test]
    fn topology_formatting() {
        assert_eq!(topology(Some(8), 8), "8");
        assert_eq!(topology(Some(4), 8), "4C/8T");
        assert_eq!(topology(None, 4), "4");
    }
}
