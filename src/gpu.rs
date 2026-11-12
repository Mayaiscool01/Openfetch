//! GPU detection through the DRM subsystem in sysfs (no root, no external tools).

use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub driver: Option<String>,
}

const PCI_IDS: &[&str] = &[
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
    "/var/lib/pciutils/pci.ids",
];

fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn read_hex(p: &Path) -> Option<u32> {
    u32::from_str_radix(read_trim(p)?.trim_start_matches("0x"), 16).ok()
}

/// Look up a PCI device name inside the text of a `pci.ids` file.
pub fn lookup_pci_ids(text: &str, vendor: u16, device: u16) -> Option<String> {
    let vprefix = format!("{vendor:04x}  ");
    let dprefix = format!("\t{device:04x}  ");
    let mut in_vendor = false;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with('\t') {
            if in_vendor {
                return None;
            }
            in_vendor = line.starts_with(&vprefix);
            continue;
        }
        if in_vendor && line.starts_with(&dprefix) {
            return Some(line[dprefix.len()..].trim().to_string());
        }
    }
    None
}

fn vendor_name(vendor: u16) -> Option<&'static str> {
    Some(match vendor {
        0x10de => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x1af4 => "Virtio",
        0x15ad => "VMware",
        0x1234 | 0x1b36 => "QEMU",
        0x80ee => "VirtualBox",
        0x1414 => "Microsoft Hyper-V",
        _ => return None,
    })
}

/// Turn a pci.ids device string into a short, readable GPU name.
/// `GA102 [GeForce RTX 3090]` becomes `NVIDIA GeForce RTX 3090`.
pub fn pretty_pci_name(vendor: u16, device_text: &str) -> String {
    let inner = match (device_text.rfind('['), device_text.rfind(']')) {
        (Some(a), Some(b)) if a < b => device_text[a + 1..b].trim(),
        _ => device_text.trim(),
    };
    match vendor_name(vendor) {
        Some(v) if !inner.starts_with(v) => format!("{v} {inner}"),
        _ => inner.to_string(),
    }
}

/// Map a device-tree compatible string of an SoC GPU to a name.
pub fn soc_gpu_name(compatible: &str, driver: &str) -> Option<String> {
    for c in compatible.split('\0').flat_map(|s| s.split(',')) {
        let name = match c.trim() {
            "agx-t8103" => "Apple M1 GPU",
            "agx-t6000" => "Apple M1 Pro GPU",
            "agx-t6001" => "Apple M1 Max GPU",
            "agx-t6002" => "Apple M1 Ultra GPU",
            "agx-t8112" => "Apple M2 GPU",
            _ => continue,
        };
        return Some(name.to_string());
    }
    match driver {
        "asahi" => Some("Apple GPU (AGX)".into()),
        "panfrost" | "panthor" => Some("Arm Mali GPU".into()),
        "v3d" | "vc4" => Some("Broadcom VideoCore GPU".into()),
        "etnaviv" => Some("Vivante GPU".into()),
        "msm" => Some("Qualcomm Adreno GPU".into()),
        "lima" => Some("Arm Mali (Utgard) GPU".into()),
        "tegra" | "nouveau" => None,
        _ => None,
    }
}

const SOC_GPU_DRIVERS: &[&str] = &[
    "asahi", "panfrost", "panthor", "v3d", "vc4", "etnaviv", "msm", "lima",
];

/// `card0`, `card12` ... but not `card0-HDMI-A-1` or `renderD128`.
fn is_card_dir(name: &str) -> bool {
    name.strip_prefix("card")
        .is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()))
}

pub fn detect() -> Vec<GpuInfo> {
    if cfg!(target_os = "macos") {
        return crate::macos::gpus();
    }
    let pci_ids = PCI_IDS.iter().find_map(|p| fs::read_to_string(p).ok());
    let mut gpus: Vec<GpuInfo> = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return gpus;
    };
    let mut cards: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| is_card_dir(&e.file_name().to_string_lossy()))
        .collect();
    cards.sort_by_key(|e| e.file_name());

    for card in cards {
        let dev = card.path().join("device");
        let driver = fs::read_link(dev.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let info = if let (Some(vendor), Some(device)) =
            (read_hex(&dev.join("vendor")), read_hex(&dev.join("device")))
        {
            // PCI device: only display controllers (class 0x03xxxx).
            if read_hex(&dev.join("class")).is_some_and(|c| c >> 16 != 0x03) {
                continue;
            }
            let (v, d) = (vendor as u16, device as u16);
            let name = pci_ids
                .as_deref()
                .and_then(|t| lookup_pci_ids(t, v, d))
                .map(|t| pretty_pci_name(v, &t))
                .unwrap_or_else(|| {
                    format!(
                        "{} GPU [{v:04x}:{d:04x}]",
                        vendor_name(v).unwrap_or("Unknown")
                    )
                });
            GpuInfo { name, driver }
        } else {
            // Platform (SoC) GPU: identify by driver / device-tree compatible.
            let drv = driver.clone().unwrap_or_default();
            if !SOC_GPU_DRIVERS.contains(&drv.as_str()) {
                continue;
            }
            let uevent = fs::read_to_string(dev.join("uevent")).unwrap_or_default();
            let compat: String = uevent
                .lines()
                .filter_map(|l| {
                    l.strip_prefix("OF_COMPATIBLE_")
                        .and_then(|r| r.split_once('='))
                })
                .map(|(_, v)| v)
                .collect::<Vec<_>>()
                .join(",");
            let Some(name) = soc_gpu_name(&compat, &drv) else {
                continue;
            };
            GpuInfo { name, driver }
        };
        if !gpus.iter().any(|g| g.name == info.name) {
            gpus.push(info);
        }
    }
    gpus
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &str = "# comment\n8086  Intel Corporation\n\t46a6  Alder Lake-P GT2 [Iris Xe Graphics]\n\t\t1028 0a62  Subsystem thing\n10de  NVIDIA Corporation\n\t2204  GA102 [GeForce RTX 3090]\n\t2488  GA104\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73ff  Navi 23 [Radeon RX 6600/6600 XT/6600M]\nC 03  Display controller\n";

    #[test]
    fn looks_up_devices() {
        assert_eq!(
            lookup_pci_ids(IDS, 0x10de, 0x2204).as_deref(),
            Some("GA102 [GeForce RTX 3090]")
        );
        assert_eq!(
            lookup_pci_ids(IDS, 0x8086, 0x46a6).as_deref(),
            Some("Alder Lake-P GT2 [Iris Xe Graphics]")
        );
        assert_eq!(lookup_pci_ids(IDS, 0x10de, 0x9999), None);
        assert_eq!(lookup_pci_ids(IDS, 0x8086, 0x2204), None);
        assert_eq!(lookup_pci_ids(IDS, 0xffff, 0x0001), None);
    }

    #[test]
    fn prettifies_names() {
        assert_eq!(
            pretty_pci_name(0x10de, "GA102 [GeForce RTX 3090]"),
            "NVIDIA GeForce RTX 3090"
        );
        assert_eq!(pretty_pci_name(0x10de, "GA104"), "NVIDIA GA104");
        assert_eq!(
            pretty_pci_name(0x1002, "Navi 23 [Radeon RX 6600/6600 XT/6600M]"),
            "AMD Radeon RX 6600/6600 XT/6600M"
        );
        assert_eq!(
            pretty_pci_name(0x8086, "Alder Lake-P GT2 [Iris Xe Graphics]"),
            "Intel Iris Xe Graphics"
        );
        assert_eq!(pretty_pci_name(0x1234, "Weird device"), "QEMU Weird device");
    }

    #[test]
    fn identifies_apple_gpus() {
        assert_eq!(
            soc_gpu_name("apple,agx-t8103\0apple,agx", "asahi").as_deref(),
            Some("Apple M1 GPU")
        );
        assert_eq!(
            soc_gpu_name("", "asahi").as_deref(),
            Some("Apple GPU (AGX)")
        );
        assert_eq!(soc_gpu_name("", "something"), None);
    }

    #[test]
    fn card_directory_filter() {
        assert!(is_card_dir("card0"));
        assert!(is_card_dir("card12"));
        assert!(!is_card_dir("card0-eDP-1"));
        assert!(!is_card_dir("renderD128"));
        assert!(!is_card_dir("card"));
    }

    #[test]
    fn detect_does_not_panic() {
        let _ = detect();
    }
}
