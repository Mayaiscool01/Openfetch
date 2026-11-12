//! RAM and swap usage.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

pub fn detect(sys: &sysinfo::System) -> MemoryInfo {
    MemoryInfo {
        total: sys.total_memory(),
        used: sys.used_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
    }
}

/// Percentage of `total` that is `used` (0.0 when `total` is zero).
pub fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0).min(100.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_handles_edges() {
        assert_eq!(percent(0, 0), 0.0);
        assert_eq!(percent(50, 200), 25.0);
        assert_eq!(percent(300, 200), 100.0);
    }

    #[test]
    fn detects_real_memory() {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let m = detect(&sys);
        assert!(m.total > 0);
        assert!(m.used <= m.total);
    }
}
