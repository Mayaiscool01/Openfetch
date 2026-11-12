//! Mounted storage devices and their usage.

use serde::Serialize;
use sysinfo::Disks;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiskInfo {
    pub device: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total: u64,
    pub used: u64,
    pub kind: String,
    pub removable: bool,
}

/// Only real, block-backed (or network) file systems are interesting.
pub fn is_real_fs(fs: &str) -> bool {
    matches!(
        fs.to_ascii_lowercase().as_str(),
        "ext2"
            | "ext3"
            | "ext4"
            | "btrfs"
            | "xfs"
            | "f2fs"
            | "zfs"
            | "bcachefs"
            | "jfs"
            | "reiserfs"
            | "vfat"
            | "fat"
            | "msdos"
            | "exfat"
            | "ntfs"
            | "ntfs3"
            | "fuseblk"
            | "apfs"
            | "hfsplus"
            | "nfs"
            | "nfs4"
            | "cifs"
            | "smb3"
            | "virtiofs"
    )
}

/// On macOS only the startup volume and mounted volumes are interesting;
/// the many internal APFS volumes under `/System/Volumes` are not.
pub fn is_user_volume(mount: &str) -> bool {
    mount == "/" || mount.starts_with("/Volumes/")
}

/// Keep one entry per device (btrfs subvolumes and bind mounts repeat),
/// preferring the shortest mount path. Result is ordered by mount point.
pub fn dedupe(mut disks: Vec<DiskInfo>) -> Vec<DiskInfo> {
    disks.sort_by(|a, b| {
        a.mount_point
            .len()
            .cmp(&b.mount_point.len())
            .then_with(|| a.mount_point.cmp(&b.mount_point))
    });
    let mut out: Vec<DiskInfo> = Vec::new();
    for d in disks {
        if !out
            .iter()
            .any(|o| o.device == d.device && o.total == d.total)
        {
            out.push(d);
        }
    }
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out
}

pub fn detect(hide_mounts: &[String]) -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let list = disks
        .list()
        .iter()
        .filter_map(|d| {
            let fs = d.file_system().to_string_lossy().into_owned();
            let device = d.name().to_string_lossy().into_owned();
            let mount = d.mount_point().to_string_lossy().into_owned();
            let total = d.total_space();
            if total == 0
                || !is_real_fs(&fs)
                || device.starts_with("/dev/loop")
                || (cfg!(target_os = "macos") && !is_user_volume(&mount))
                || hide_mounts.iter().any(|h| h == &mount)
                || [
                    "/snap",
                    "/var/snap",
                    "/var/lib/docker",
                    "/var/lib/containers",
                ]
                .iter()
                .any(|p| mount.starts_with(p))
            {
                return None;
            }
            Some(DiskInfo {
                device,
                mount_point: mount,
                fs_type: fs,
                total,
                used: total.saturating_sub(d.available_space()),
                kind: match d.kind() {
                    sysinfo::DiskKind::SSD => "SSD".into(),
                    sysinfo::DiskKind::HDD => "HDD".into(),
                    _ => String::new(),
                },
                removable: d.is_removable(),
            })
        })
        .collect();
    dedupe(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(device: &str, mount: &str, total: u64) -> DiskInfo {
        DiskInfo {
            device: device.into(),
            mount_point: mount.into(),
            fs_type: "btrfs".into(),
            total,
            used: total / 2,
            kind: String::new(),
            removable: false,
        }
    }

    #[test]
    fn filters_pseudo_filesystems() {
        for good in ["ext4", "btrfs", "xfs", "vfat", "ntfs3", "NFS4"] {
            assert!(is_real_fs(good), "{good}");
        }
        for bad in [
            "tmpfs", "devtmpfs", "overlay", "squashfs", "proc", "efivarfs", "",
        ] {
            assert!(!is_real_fs(bad), "{bad}");
        }
    }

    #[test]
    fn macos_volume_filter() {
        assert!(is_user_volume("/"));
        assert!(is_user_volume("/Volumes/Backup"));
        assert!(!is_user_volume("/System/Volumes/Data"));
        assert!(!is_user_volume("/System/Volumes/VM"));
    }

    #[test]
    fn btrfs_subvolumes_collapse_to_one_entry() {
        let out = dedupe(vec![
            disk("/dev/nvme0n1p5", "/home", 1000),
            disk("/dev/nvme0n1p5", "/", 1000),
            disk("/dev/nvme0n1p5", "/var/log", 1000),
            disk("/dev/sda1", "/mnt/data", 2000),
        ]);
        let mounts: Vec<_> = out.iter().map(|d| d.mount_point.as_str()).collect();
        assert_eq!(mounts, vec!["/", "/mnt/data"]);
    }

    #[test]
    fn detect_does_not_panic() {
        for d in detect(&[]) {
            assert!(d.used <= d.total);
        }
    }
}
