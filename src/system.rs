//! Collects everything into one [`SystemInfo`] value; also hosts firmware,
//! motherboard and battery detection.

use crate::{cpu, desktop, gpu, memory, network, os, storage};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

/// Everything that can appear in the `modules` list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleKind {
    Title,
    Rule,
    Break,
    Colors,
    Os,
    Host,
    Kernel,
    Uptime,
    Packages,
    Shell,
    Terminal,
    De,
    Wm,
    Resolution,
    Motherboard,
    Bios,
    Cpu,
    CpuUsage,
    CpuTemp,
    Gpu,
    Memory,
    Swap,
    Disk,
    Battery,
    Network,
    LocalIp,
    Theme,
    Icons,
    Cursor,
    Font,
    Load,
    Processes,
    Init,
    Locale,
}

impl ModuleKind {
    pub const ALL: [ModuleKind; 34] = [
        ModuleKind::Title,
        ModuleKind::Rule,
        ModuleKind::Break,
        ModuleKind::Colors,
        ModuleKind::Os,
        ModuleKind::Host,
        ModuleKind::Kernel,
        ModuleKind::Uptime,
        ModuleKind::Packages,
        ModuleKind::Shell,
        ModuleKind::Terminal,
        ModuleKind::De,
        ModuleKind::Wm,
        ModuleKind::Resolution,
        ModuleKind::Motherboard,
        ModuleKind::Bios,
        ModuleKind::Cpu,
        ModuleKind::CpuUsage,
        ModuleKind::CpuTemp,
        ModuleKind::Gpu,
        ModuleKind::Memory,
        ModuleKind::Swap,
        ModuleKind::Disk,
        ModuleKind::Battery,
        ModuleKind::Network,
        ModuleKind::LocalIp,
        ModuleKind::Theme,
        ModuleKind::Icons,
        ModuleKind::Cursor,
        ModuleKind::Font,
        ModuleKind::Load,
        ModuleKind::Processes,
        ModuleKind::Init,
        ModuleKind::Locale,
    ];

    /// Parse a module name from the config file (a few aliases are accepted).
    pub fn from_name(name: &str) -> Option<Self> {
        use ModuleKind::*;
        let n = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Some(match n.as_str() {
            "title" => Title,
            "rule" | "line" | "underline" => Rule,
            "break" | "blank" | "separator" => Break,
            "colors" | "colours" | "palette" => Colors,
            "os" | "distro" => Os,
            "host" | "model" => Host,
            "kernel" => Kernel,
            "uptime" => Uptime,
            "packages" | "pkgs" => Packages,
            "shell" => Shell,
            "terminal" | "term" => Terminal,
            "de" | "desktop" => De,
            "wm" | "window_manager" => Wm,
            "resolution" | "display" => Resolution,
            "motherboard" | "board" => Motherboard,
            "bios" | "firmware" | "uefi" => Bios,
            "cpu" => Cpu,
            "cpu_usage" => CpuUsage,
            "cpu_temp" | "cpu_temperature" | "temperature" => CpuTemp,
            "gpu" => Gpu,
            "memory" | "ram" => Memory,
            "swap" => Swap,
            "disk" | "storage" => Disk,
            "battery" => Battery,
            "network" | "net" => Network,
            "localip" | "local_ip" | "ip" => LocalIp,
            "theme" | "gtk_theme" | "style" => Theme,
            "icons" | "icon_theme" => Icons,
            "cursor" => Cursor,
            "font" => Font,
            "load" | "loadavg" | "load_average" => Load,
            "processes" | "procs" => Processes,
            "init" | "init_system" => Init,
            "locale" | "lang" => Locale,
            _ => return None,
        })
    }

    /// Key used for `[labels]` overrides.
    pub fn key(self) -> &'static str {
        use ModuleKind::*;
        match self {
            Title => "title",
            Rule => "rule",
            Break => "break",
            Colors => "colors",
            Os => "os",
            Host => "host",
            Kernel => "kernel",
            Uptime => "uptime",
            Packages => "packages",
            Shell => "shell",
            Terminal => "terminal",
            De => "de",
            Wm => "wm",
            Resolution => "resolution",
            Motherboard => "motherboard",
            Bios => "bios",
            Cpu => "cpu",
            CpuUsage => "cpu_usage",
            CpuTemp => "cpu_temp",
            Gpu => "gpu",
            Memory => "memory",
            Swap => "swap",
            Disk => "disk",
            Battery => "battery",
            Network => "network",
            LocalIp => "localip",
            Theme => "theme",
            Icons => "icons",
            Cursor => "cursor",
            Font => "font",
            Load => "load",
            Processes => "processes",
            Init => "init",
            Locale => "locale",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Firmware {
    pub vendor: Option<String>,
    pub version: Option<String>,
    pub date: Option<String>,
    /// `UEFI` or `BIOS`.
    pub mode: String,
    pub secure_boot: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatteryInfo {
    pub name: String,
    pub percent: Option<u8>,
    pub status: String,
    /// Full charge capacity relative to design capacity.
    pub health_percent: Option<f32>,
    pub cycles: Option<u32>,
    pub minutes_remaining: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SystemInfo {
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub os: Option<os::OsRelease>,
    pub host: Option<String>,
    pub kernel: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub packages: Vec<os::PackageCount>,
    pub shell: Option<String>,
    pub terminal: Option<String>,
    pub desktop: Option<desktop::DesktopInfo>,
    pub displays: Vec<desktop::Display>,
    pub motherboard: Option<String>,
    pub firmware: Option<Firmware>,
    pub cpu: Option<cpu::CpuInfo>,
    pub gpus: Vec<gpu::GpuInfo>,
    pub memory: Option<memory::MemoryInfo>,
    pub disks: Vec<storage::DiskInfo>,
    pub batteries: Vec<BatteryInfo>,
    pub network: Option<network::NetworkInfo>,
    pub appearance: Option<desktop::Appearance>,
    pub load_average: Option<[f32; 3]>,
    pub processes: Option<usize>,
    pub init: Option<String>,
    pub locale: Option<String>,
}

// ------------------------------------------------------------ sysfs helpers

fn read_trim(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().trim_end_matches('\0').trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_num<T: std::str::FromStr>(p: impl AsRef<Path>) -> Option<T> {
    read_trim(p)?.parse().ok()
}

/// Drop empty values and the placeholder strings OEMs leave in DMI tables.
pub fn clean_dmi(v: &str) -> Option<String> {
    let t = v.trim();
    let l = t.to_ascii_lowercase();
    const JUNK: &[&str] = &[
        "",
        "to be filled by o.e.m.",
        "to be filled by o.e.m",
        "default string",
        "system product name",
        "system version",
        "system manufacturer",
        "not specified",
        "not applicable",
        "none",
        "n/a",
        "unknown",
        "o.e.m.",
        "base board product name",
        "type1productconfigid",
    ];
    (!JUNK.contains(&l.as_str())).then(|| t.to_string())
}

fn dmi(field: &str) -> Option<String> {
    read_trim(format!("/sys/class/dmi/id/{field}")).and_then(|v| clean_dmi(&v))
}

fn device_tree_model() -> Option<String> {
    read_trim("/proc/device-tree/model")
        .or_else(|| read_trim("/sys/firmware/devicetree/base/model"))
}

/// First entry of the device-tree `compatible` list (e.g. `apple,j313`).
fn device_tree_board() -> Option<String> {
    let raw = fs::read("/proc/device-tree/compatible").ok()?;
    raw.split(|b| *b == 0)
        .next()
        .map(|s| String::from_utf8_lossy(s).trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn host_model() -> Option<String> {
    let name = dmi("product_name");
    let version = dmi("product_version");
    let vendor = dmi("sys_vendor").unwrap_or_default();
    // Lenovo stores the human-readable model in product_version.
    if vendor.to_ascii_lowercase().contains("lenovo") {
        if let Some(v) = version {
            return Some(v);
        }
    }
    match (name, version) {
        (Some(n), Some(v)) if v != n && v.len() < 40 && !v.starts_with("None") => {
            Some(format!("{n} ({v})"))
        }
        (Some(n), _) => Some(n),
        _ => device_tree_model(),
    }
}

pub fn motherboard() -> Option<String> {
    match (dmi("board_vendor"), dmi("board_name")) {
        (Some(v), Some(n)) => Some(format!("{v} {n}")),
        (None, Some(n)) => Some(n),
        _ => device_tree_board(),
    }
}

pub fn firmware() -> Option<Firmware> {
    let efi = Path::new("/sys/firmware/efi").exists();
    let vendor = dmi("bios_vendor");
    let version = dmi("bios_version");
    let date = dmi("bios_date");
    if !efi && vendor.is_none() && version.is_none() {
        return None;
    }
    let secure_boot = if efi {
        fs::read("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c")
            .ok()
            .and_then(|b| b.last().map(|v| *v == 1))
    } else {
        None
    };
    Some(Firmware {
        vendor,
        version,
        date,
        mode: if efi { "UEFI" } else { "BIOS" }.to_string(),
        secure_boot,
    })
}

/// Minutes until empty (discharging) or full (charging), if computable.
pub fn estimate_minutes(status: &str, now: f64, full: f64, rate: f64) -> Option<u32> {
    if rate <= 0.0 {
        return None;
    }
    let hours = match status {
        "Discharging" => now / rate,
        "Charging" => (full - now).max(0.0) / rate,
        _ => return None,
    };
    let minutes = hours * 60.0;
    (minutes.is_finite() && minutes < 60.0 * 99.0).then_some(minutes.round() as u32)
}

pub fn batteries() -> Vec<BatteryInfo> {
    let Ok(dir) = fs::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    let mut list: Vec<_> = dir.filter_map(Result::ok).collect();
    list.sort_by_key(|e| e.file_name());
    list.into_iter()
        .filter_map(|e| {
            let p = e.path();
            if read_trim(p.join("type"))? != "Battery" {
                return None;
            }
            // Skip peripherals (mice, keyboards) exposed as batteries.
            if read_trim(p.join("scope")).as_deref() == Some("Device") {
                return None;
            }
            let status = read_trim(p.join("status")).unwrap_or_else(|| "Unknown".into());
            let pair = |a: &str, b: &str| -> Option<(f64, f64)> {
                Some((read_num(p.join(a))?, read_num(p.join(b))?))
            };
            let (now, full, design, rate) = if let Some((n, f)) = pair("energy_now", "energy_full")
            {
                (
                    n,
                    f,
                    read_num(p.join("energy_full_design")),
                    read_num(p.join("power_now")),
                )
            } else if let Some((n, f)) = pair("charge_now", "charge_full") {
                (
                    n,
                    f,
                    read_num(p.join("charge_full_design")),
                    read_num(p.join("current_now")),
                )
            } else {
                (0.0, 0.0, None, None)
            };
            let percent = read_num::<u8>(p.join("capacity")).map(|c| c.min(100));
            let health = design
                .filter(|d: &f64| *d > 0.0 && full > 0.0)
                .map(|d| ((full / d * 100.0) as f32).min(100.0));
            Some(BatteryInfo {
                name: e.file_name().to_string_lossy().into_owned(),
                percent,
                health_percent: health,
                cycles: read_num::<u32>(p.join("cycle_count")).filter(|c| *c > 0),
                minutes_remaining: rate.and_then(|r: f64| estimate_minutes(&status, now, full, r)),
                status,
            })
        })
        .collect()
}

// ------------------------------------------------------------------ collect

pub struct CollectOptions {
    pub sample_cpu: bool,
    pub hide_mounts: Vec<String>,
}

impl Default for CollectOptions {
    fn default() -> Self {
        CollectOptions {
            sample_cpu: true,
            hide_mounts: vec!["/boot/efi".into(), "/efi".into()],
        }
    }
}

/// Collect only what the requested modules need.
pub fn collect(wanted: &HashSet<ModuleKind>, opts: &CollectOptions) -> SystemInfo {
    use ModuleKind::*;
    let w = |k: ModuleKind| wanted.contains(&k);
    let env = |k: &str| std::env::var(k).ok();
    let mut info = SystemInfo {
        username: os::username(),
        hostname: os::hostname(),
        os: Some(os::detect_os()), // always: needed to choose the logo
        ..Default::default()
    };

    // Start the CPU usage sample first so other work overlaps its interval.
    let want_cpu = w(Cpu) || w(CpuUsage) || w(CpuTemp);
    let want_usage = want_cpu && opts.sample_cpu && (w(Cpu) || w(CpuUsage));
    let mut sys = sysinfo::System::new();
    let started = Instant::now();
    if want_cpu {
        sys.refresh_cpu_all();
    }

    if w(Host) {
        info.host = host_model();
    }
    if w(Kernel) {
        info.kernel = os::kernel();
    }
    if w(Uptime) {
        info.uptime_seconds = os::uptime_seconds();
    }
    if w(Packages) {
        info.packages = os::packages();
    }
    if w(Shell) {
        info.shell = os::shell_with_version();
    }
    if w(Terminal) {
        info.terminal = os::terminal();
    }
    if w(De) || w(Wm) || w(Theme) || w(Icons) || w(Cursor) || w(Font) {
        info.desktop = Some(desktop::detect(&env));
    }
    if w(Theme) || w(Icons) || w(Cursor) || w(Font) {
        let home = std::path::PathBuf::from(env("HOME").unwrap_or_default());
        let de = info.desktop.as_ref().and_then(|d| d.de.clone());
        info.appearance = Some(desktop::detect_appearance(&home, de.as_deref()));
    }
    if w(Load) {
        info.load_average = os::load_average();
    }
    if w(Processes) {
        info.processes = os::process_count();
    }
    if w(Init) {
        info.init = os::init_system();
    }
    if w(Locale) {
        info.locale = os::locale();
    }
    if w(Resolution) {
        info.displays = desktop::displays();
    }
    if w(Motherboard) {
        info.motherboard = motherboard();
    }
    if w(Bios) {
        info.firmware = firmware();
    }
    if w(Gpu) {
        info.gpus = gpu::detect();
    }
    if w(Memory) || w(Swap) {
        sys.refresh_memory();
        info.memory = Some(memory::detect(&sys));
    }
    if w(Disk) {
        info.disks = storage::detect(&opts.hide_mounts);
    }
    if w(Battery) {
        info.batteries = batteries();
    }
    if w(Network) || w(LocalIp) {
        info.network = Some(network::detect());
    }

    if want_cpu {
        let mut usage = None;
        if want_usage {
            let wait = sysinfo::MINIMUM_CPU_UPDATE_INTERVAL + Duration::from_millis(20);
            if let Some(rest) = wait.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
            sys.refresh_cpu_usage();
            usage = Some(sys.global_cpu_usage());
        }
        info.cpu = Some(cpu::detect(&sys, usage, w(CpuTemp) || w(Cpu)));
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names_and_aliases() {
        assert_eq!(ModuleKind::from_name("OS"), Some(ModuleKind::Os));
        assert_eq!(
            ModuleKind::from_name("window-manager"),
            Some(ModuleKind::Wm)
        );
        assert_eq!(ModuleKind::from_name("ram"), Some(ModuleKind::Memory));
        assert_eq!(ModuleKind::from_name("storage"), Some(ModuleKind::Disk));
        assert_eq!(ModuleKind::from_name("nonsense"), None);
    }

    #[test]
    fn every_module_key_round_trips() {
        for kind in ModuleKind::ALL {
            assert_eq!(ModuleKind::from_name(kind.key()), Some(kind), "{kind:?}");
        }
        let unique: HashSet<_> = ModuleKind::ALL.iter().collect();
        assert_eq!(unique.len(), ModuleKind::ALL.len());
    }

    #[test]
    fn dmi_placeholders_are_dropped() {
        assert_eq!(clean_dmi("To Be Filled By O.E.M."), None);
        assert_eq!(clean_dmi("  Default string "), None);
        assert_eq!(clean_dmi(""), None);
        assert_eq!(
            clean_dmi("ASUSTeK COMPUTER INC.").as_deref(),
            Some("ASUSTeK COMPUTER INC.")
        );
    }

    #[test]
    fn battery_time_estimates() {
        // 30 Wh left at 15 W -> 2 hours
        assert_eq!(estimate_minutes("Discharging", 30.0, 50.0, 15.0), Some(120));
        // 20 Wh to fill at 10 W -> 2 hours
        assert_eq!(estimate_minutes("Charging", 30.0, 50.0, 10.0), Some(120));
        assert_eq!(estimate_minutes("Full", 50.0, 50.0, 10.0), None);
        assert_eq!(estimate_minutes("Discharging", 30.0, 50.0, 0.0), None);
    }

    #[test]
    fn collect_everything_on_this_machine() {
        let wanted: HashSet<_> = ModuleKind::ALL.into_iter().collect();
        let info = collect(&wanted, &CollectOptions::default());
        assert!(info.os.is_some());
        assert!(info.kernel.is_some());
        assert!(info.cpu.as_ref().is_some_and(|c| c.threads >= 1));
        assert!(info.memory.as_ref().is_some_and(|m| m.total > 0));
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"kernel\""));
    }

    #[test]
    fn collect_only_what_is_wanted() {
        let wanted: HashSet<_> = [ModuleKind::Kernel].into_iter().collect();
        let info = collect(&wanted, &CollectOptions::default());
        assert!(info.kernel.is_some());
        assert!(info.cpu.is_none() && info.memory.is_none() && info.gpus.is_empty());
    }
}
