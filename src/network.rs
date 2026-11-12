//! Network interfaces and local IP addresses (offline: reads only local state).

use serde::Serialize;
use std::fs;
use std::net::IpAddr;
use sysinfo::Networks;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Interface {
    pub name: String,
    pub up: bool,
    pub mac: Option<String>,
    /// Addresses with prefix length, IPv4 first (`192.168.1.20/24`).
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetworkInfo {
    pub interfaces: Vec<Interface>,
    /// Interface carrying the default route.
    pub primary: Option<String>,
    pub local_ip: Option<String>,
}

/// Container/VM plumbing interfaces that just add noise.
pub fn is_virtual(name: &str) -> bool {
    name == "lo"
        || name == "lo0"
        || [
            "veth", "docker", "br-", "virbr", "vnet", "cni", "flannel", "podman",
            // macOS tunnels and internal links
            "utun", "awdl", "llw", "gif", "stf", "anpi", "ap1", "bridge",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}

/// Default-route interface from the contents of `/proc/net/route`.
pub fn default_route_interface(route: &str) -> Option<String> {
    route.lines().skip(1).find_map(|l| {
        let mut f = l.split_whitespace();
        let iface = f.next()?;
        let dest = f.next()?;
        let flags = u32::from_str_radix(f.nth(1)?, 16).ok()?;
        (dest == "00000000" && flags & 0x2 != 0).then(|| iface.to_string())
    })
}

fn is_link_local_v6(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80)
}

pub fn detect() -> NetworkInfo {
    let networks = Networks::new_with_refreshed_list();
    let mut interfaces: Vec<Interface> = networks
        .iter()
        .filter(|(name, _)| !is_virtual(name))
        .map(|(name, data)| {
            let state =
                fs::read_to_string(format!("/sys/class/net/{name}/operstate")).unwrap_or_default();
            // Without sysfs (macOS) an interface counts as up when it has an address.
            let linux_up = matches!(state.trim(), "up" | "unknown");
            let mut v4 = Vec::new();
            let mut v6 = Vec::new();
            for net in data.ip_networks() {
                let s = format!("{}/{}", net.addr, net.prefix);
                match net.addr {
                    IpAddr::V4(_) => v4.push(s),
                    ip if !is_link_local_v6(&ip) => v6.push(s),
                    _ => {}
                }
            }
            v4.extend(v6);
            let mac = data.mac_address().to_string();
            Interface {
                name: name.clone(),
                up: if cfg!(target_os = "linux") {
                    linux_up
                } else {
                    !v4.is_empty()
                },
                mac: (mac != "00:00:00:00:00:00").then_some(mac),
                addresses: v4,
            }
        })
        .collect();
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    let default_route = if cfg!(target_os = "macos") {
        crate::macos::default_interface()
    } else {
        fs::read_to_string("/proc/net/route")
            .ok()
            .and_then(|r| default_route_interface(&r))
    };
    let primary = default_route
        .filter(|p| interfaces.iter().any(|i| &i.name == p))
        .or_else(|| {
            interfaces
                .iter()
                .find(|i| i.up && i.addresses.iter().any(|a| !a.contains(':')))
                .map(|i| i.name.clone())
        });
    let local_ip = primary
        .as_ref()
        .and_then(|p| interfaces.iter().find(|i| &i.name == p))
        .and_then(|i| i.addresses.iter().find(|a| !a.contains(':')))
        .map(|a| a.split('/').next().unwrap_or(a).to_string());
    NetworkInfo {
        interfaces,
        primary,
        local_ip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\nwlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\nwlan0\t0001A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n";

    #[test]
    fn finds_default_route() {
        assert_eq!(default_route_interface(ROUTE).as_deref(), Some("wlan0"));
        assert_eq!(default_route_interface("Iface\tDestination\n"), None);
        assert_eq!(default_route_interface(""), None);
    }

    #[test]
    fn hides_virtual_interfaces() {
        for v in [
            "lo", "lo0", "veth1234", "docker0", "br-abc", "virbr0", "utun3", "awdl0",
        ] {
            assert!(is_virtual(v), "{v}");
        }
        for r in [
            "eth0",
            "wlan0",
            "enp3s0",
            "wlp2s0",
            "tailscale0",
            "en0",
            "en1",
        ] {
            assert!(!is_virtual(r), "{r}");
        }
    }

    #[test]
    fn link_local_v6_detection() {
        assert!(is_link_local_v6(&"fe80::1".parse().unwrap()));
        assert!(!is_link_local_v6(&"2001:db8::1".parse().unwrap()));
        assert!(!is_link_local_v6(&"192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn detect_does_not_panic() {
        let n = detect();
        assert!(n
            .interfaces
            .iter()
            .all(|i| i.name != "lo" && i.name != "lo0"));
    }
}
