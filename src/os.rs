//! Operating system details: distribution, kernel, hostname, uptime, shell,
//! terminal emulator and installed package counts.

use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct OsRelease {
    pub name: String,
    pub pretty_name: String,
    pub version: Option<String>,
    pub id: String,
    pub id_like: Vec<String>,
    pub arch: String,
}

fn unquote(v: &str) -> String {
    let b = v.as_bytes();
    let quoted = v.len() >= 2
        && ((b[0] == b'"' && b[v.len() - 1] == b'"') || (b[0] == b'\'' && b[v.len() - 1] == b'\''));
    if !quoted {
        return v.to_string();
    }
    let inner = &v[1..v.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Parse the contents of an `os-release` file.
pub fn parse_os_release(text: &str) -> OsRelease {
    let (mut name, mut pretty, mut version, mut version_id) = (None, None, None, None);
    let (mut id, mut id_like) = (None, Vec::new());
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = unquote(v.trim());
        match k.trim() {
            "NAME" => name = Some(v),
            "PRETTY_NAME" => pretty = Some(v),
            "VERSION" => version = Some(v),
            "VERSION_ID" => version_id = Some(v),
            "ID" => id = Some(v.to_ascii_lowercase()),
            "ID_LIKE" => {
                id_like = v
                    .split_whitespace()
                    .map(|s| s.to_ascii_lowercase())
                    .collect()
            }
            _ => {}
        }
    }
    let name = name.unwrap_or_else(|| "Linux".to_string());
    let version = version_id.or(version).filter(|v| !v.is_empty());
    let pretty_name = pretty
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| match &version {
            Some(v) => format!("{name} {v}"),
            None => name.clone(),
        });
    OsRelease {
        name,
        pretty_name,
        version,
        id: id.unwrap_or_else(|| "linux".to_string()),
        id_like,
        arch: env::consts::ARCH.to_string(),
    }
}

pub fn detect_os() -> OsRelease {
    for p in ["/etc/os-release", "/usr/lib/os-release"] {
        if let Ok(text) = fs::read_to_string(p) {
            return parse_os_release(&text);
        }
    }
    parse_os_release("")
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn kernel() -> Option<String> {
    read_trim("/proc/sys/kernel/osrelease")
}

pub fn hostname() -> Option<String> {
    read_trim("/proc/sys/kernel/hostname")
        .or_else(|| read_trim("/etc/hostname"))
        .or_else(|| env::var("HOSTNAME").ok().filter(|h| !h.is_empty()))
}

pub fn username() -> Option<String> {
    env::var("USER")
        .ok()
        .or_else(|| env::var("LOGNAME").ok())
        .filter(|u| !u.is_empty())
}

pub fn uptime_seconds() -> Option<u64> {
    let text = fs::read_to_string("/proc/uptime").ok()?;
    let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(secs as u64)
}

/// Name of the login shell (`$SHELL`), without its path.
pub fn shell() -> Option<String> {
    let s = env::var("SHELL").ok()?;
    let base = s.rsplit('/').next().unwrap_or(&s).to_string();
    (!base.is_empty()).then_some(base)
}

/// Pull a version like `5.3.3` out of `--version` output (`GNU bash, version 5.3.3(1)-release`).
pub fn parse_shell_version(output: &str) -> Option<String> {
    output
        .lines()
        .next()?
        .split_whitespace()
        .find(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains('.'))
        .map(|t| {
            t.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .trim_end_matches('.')
                .to_string()
        })
        .filter(|v| !v.is_empty())
}

/// Shell name plus its version when it can be asked for cheaply (`bash 5.3.3`).
pub fn shell_with_version() -> Option<String> {
    let name = shell()?;
    if !["bash", "zsh", "fish", "nu"].contains(&name.as_str()) {
        return Some(name);
    }
    let path = env::var("SHELL").ok()?;
    let version =
        command_lines(&path, &["--version"]).and_then(|l| parse_shell_version(&l.join("\n")));
    Some(match version {
        Some(v) => format!("{name} {v}"),
        None => name,
    })
}

/// 1, 5 and 15 minute load averages.
pub fn load_average() -> Option<[f32; 3]> {
    parse_loadavg(&fs::read_to_string("/proc/loadavg").ok()?)
}

pub fn parse_loadavg(text: &str) -> Option<[f32; 3]> {
    let mut it = text.split_whitespace().map(|v| v.parse::<f32>().ok());
    Some([it.next()??, it.next()??, it.next()??])
}

/// Number of running processes.
pub fn process_count() -> Option<usize> {
    let n = fs::read_dir("/proc")
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        })
        .count();
    (n > 0).then_some(n)
}

/// Name of PID 1 (`systemd`, `openrc-init`, `runit`, ...).
pub fn init_system() -> Option<String> {
    read_trim("/proc/1/comm")
}

pub fn locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|v| !v.is_empty()))
}

// ---------------------------------------------------------------- terminal

/// Parent PID and process name of `pid`, read from `/proc/<pid>/stat`.
fn ppid_and_comm(pid: u32) -> Option<(u32, String)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_stat(&stat)
}

fn parse_stat(stat: &str) -> Option<(u32, String)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_string();
    let rest = stat.get(close + 1..)?;
    let ppid = rest.split_whitespace().nth(1)?.parse().ok()?;
    Some((ppid, comm))
}

const TERMINALS: &[(&str, &str)] = &[
    ("gnome-terminal", "GNOME Terminal"),
    ("ptyxis", "Ptyxis"),
    ("kgx", "GNOME Console"),
    ("konsole", "Konsole"),
    ("yakuake", "Yakuake"),
    ("kitty", "kitty"),
    ("alacritty", "Alacritty"),
    ("wezterm", "WezTerm"),
    ("foot", "foot"),
    ("ghostty", "Ghostty"),
    ("rio", "Rio"),
    ("xfce4-terminal", "Xfce Terminal"),
    ("lxterminal", "LXTerminal"),
    ("qterminal", "QTerminal"),
    ("mate-terminal", "MATE Terminal"),
    ("tilix", "Tilix"),
    ("terminator", "Terminator"),
    ("sakura", "sakura"),
    ("urxvt", "rxvt-unicode"),
    ("xterm", "xterm"),
    ("st", "st"),
    ("tmux", "tmux"),
    ("screen", "screen"),
];

fn terminal_from_comm(comm: &str) -> Option<&'static str> {
    let c = comm.to_ascii_lowercase();
    TERMINALS
        .iter()
        .find(|(prefix, _)| {
            if prefix.len() <= 2 {
                c == *prefix
            } else {
                c.starts_with(prefix)
            }
        })
        .map(|(_, pretty)| *pretty)
}

pub fn terminal() -> Option<String> {
    let env_name = |k: &str| env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(tp) = env_name("TERM_PROGRAM") {
        let pretty = match tp.as_str() {
            "vscode" => "VS Code".to_string(),
            "Apple_Terminal" => "Terminal".to_string(),
            "iTerm.app" => "iTerm2".to_string(),
            other => other.to_string(),
        };
        return Some(match env_name("TERM_PROGRAM_VERSION") {
            Some(v) if tp != "tmux" => format!("{pretty} {v}"),
            _ => pretty,
        });
    }
    for (var, name) in [
        ("KITTY_WINDOW_ID", "kitty"),
        ("ALACRITTY_SOCKET", "Alacritty"),
        ("ALACRITTY_LOG", "Alacritty"),
        ("WEZTERM_EXECUTABLE", "WezTerm"),
        ("KONSOLE_VERSION", "Konsole"),
        ("GHOSTTY_RESOURCES_DIR", "Ghostty"),
        ("TILIX_ID", "Tilix"),
        ("WT_SESSION", "Windows Terminal"),
    ] {
        if env_name(var).is_some() {
            return Some(name.to_string());
        }
    }
    // Walk up the process tree looking for a known terminal emulator.
    let mut pid = std::process::id();
    for _ in 0..12 {
        let Some((ppid, _)) = ppid_and_comm(pid) else {
            break;
        };
        if ppid <= 1 {
            break;
        }
        if let Some((_, comm)) = ppid_and_comm(ppid) {
            if let Some(t) = terminal_from_comm(&comm) {
                return Some(t.to_string());
            }
        }
        pid = ppid;
    }
    if env_name("TMUX").is_some() {
        return Some("tmux".into());
    }
    if env_name("VTE_VERSION").is_some() {
        return Some("VTE-based terminal".into());
    }
    env_name("TERM")
}

// ---------------------------------------------------------------- packages

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageCount {
    pub manager: String,
    pub count: usize,
}

pub fn count_dpkg(status: &str) -> usize {
    status
        .lines()
        .filter(|l| *l == "Status: install ok installed")
        .count()
}

pub fn count_apk(db: &str) -> usize {
    db.lines().filter(|l| l.starts_with("P:")).count()
}

fn count_dirs(path: impl AsRef<Path>) -> usize {
    fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

fn command_lines(cmd: &str, args: &[&str]) -> Option<Vec<String>> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    })
}

pub fn packages() -> Vec<PackageCount> {
    let mut out = Vec::new();
    let mut add = |manager: &str, count: usize| {
        if count > 0 {
            out.push(PackageCount {
                manager: manager.to_string(),
                count,
            });
        }
    };

    if let Ok(status) = fs::read_to_string("/var/lib/dpkg/status") {
        add("dpkg", count_dpkg(&status));
    }
    add("pacman", count_dirs("/var/lib/pacman/local"));
    let rpm_db = [
        "/var/lib/rpm/rpmdb.sqlite",
        "/usr/lib/sysimage/rpm/rpmdb.sqlite",
        "/usr/lib/sysimage/rpm/Packages.db",
        "/var/lib/rpm/Packages",
    ];
    if rpm_db.iter().any(|p| Path::new(p).exists()) {
        if let Some(lines) = command_lines(
            "rpm",
            &["-qa", "--nosignature", "--nodigest", "--qf", "%{NAME}\\n"],
        ) {
            add(
                "rpm",
                lines
                    .iter()
                    .filter(|l| !l.starts_with("gpg-pubkey"))
                    .count(),
            );
        }
    }
    if let Ok(db) = fs::read_to_string("/lib/apk/db/installed") {
        add("apk", count_apk(&db));
    }
    if Path::new("/var/db/xbps").exists() {
        if let Some(lines) = command_lines("xbps-query", &["-l"]) {
            add("xbps", lines.len());
        }
    }
    let mut flatpak = count_dirs("/var/lib/flatpak/app");
    if let Some(home) = env::var_os("HOME") {
        flatpak += count_dirs(Path::new(&home).join(".local/share/flatpak/app"));
    }
    add("flatpak", flatpak);
    let snaps = fs::read_dir("/var/lib/snapd/snaps")
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    n.ends_with(".snap")
                        && !["core", "snapd", "bare"].iter().any(|p| n.starts_with(p))
                })
                .count()
        })
        .unwrap_or(0);
    add("snap", snaps);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEDORA_ASAHI: &str = r#"NAME="Fedora Linux"
VERSION="41 (Asahi Remix)"
RELEASE_TYPE=stable
ID=fedora
VERSION_ID=41
PRETTY_NAME="Fedora Linux 41 (Asahi Remix)"
"#;

    #[test]
    fn parses_fedora() {
        let os = parse_os_release(FEDORA_ASAHI);
        assert_eq!(os.id, "fedora");
        assert_eq!(os.name, "Fedora Linux");
        assert_eq!(os.version.as_deref(), Some("41"));
        assert_eq!(os.pretty_name, "Fedora Linux 41 (Asahi Remix)");
    }

    #[test]
    fn parses_debian_ubuntu_arch_suse() {
        let debian = parse_os_release("PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\nNAME=\"Debian GNU/Linux\"\nVERSION_ID=\"12\"\nID=debian\n");
        assert_eq!(debian.id, "debian");
        let ubuntu = parse_os_release("NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nID=ubuntu\nID_LIKE=debian\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n");
        assert_eq!(ubuntu.id_like, vec!["debian"]);
        let arch = parse_os_release(
            "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\nBUILD_ID=rolling\n",
        );
        assert_eq!(arch.pretty_name, "Arch Linux");
        assert_eq!(arch.version, None);
        let suse = parse_os_release(
            "NAME=\"openSUSE Tumbleweed\"\nID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n",
        );
        assert_eq!(suse.id, "opensuse-tumbleweed");
        assert_eq!(suse.id_like, vec!["opensuse", "suse"]);
    }

    #[test]
    fn handles_missing_and_odd_input() {
        let os = parse_os_release("");
        assert_eq!(os.id, "linux");
        assert_eq!(os.pretty_name, "Linux");
        let os = parse_os_release("# comment\nNAME='Alpine Linux'\nVERSION_ID=3.20\nGARBAGE\n");
        assert_eq!(os.pretty_name, "Alpine Linux 3.20");
        assert_eq!(unquote("\"a\\\"b\""), "a\"b");
    }

    #[test]
    fn parses_proc_stat_with_awkward_names() {
        let (ppid, comm) = parse_stat("1234 (my (weird) name) S 99 1234 1234 0 -1").unwrap();
        assert_eq!(ppid, 99);
        assert_eq!(comm, "my (weird) name");
        assert!(parse_stat("garbage").is_none());
    }

    #[test]
    fn recognises_terminal_processes() {
        assert_eq!(
            terminal_from_comm("gnome-terminal-"),
            Some("GNOME Terminal")
        );
        assert_eq!(terminal_from_comm("konsole"), Some("Konsole"));
        assert_eq!(terminal_from_comm("st"), Some("st"));
        assert_eq!(terminal_from_comm("sshd"), None);
        assert_eq!(terminal_from_comm("bash"), None);
    }

    #[test]
    fn counts_packages() {
        let status = "Package: a\nStatus: install ok installed\n\nPackage: b\nStatus: deinstall ok config-files\n\nPackage: c\nStatus: install ok installed\n";
        assert_eq!(count_dpkg(status), 2);
        assert_eq!(count_apk("P:musl\nV:1\n\nP:busybox\nV:2\n"), 2);
    }

    #[test]
    fn shell_versions() {
        assert_eq!(
            parse_shell_version(
                "GNU bash, version 5.3.3(1)-release (x86_64-pc-linux-gnu)\nCopyright"
            )
            .as_deref(),
            Some("5.3.3")
        );
        assert_eq!(
            parse_shell_version("zsh 5.9 (x86_64-ubuntu-linux-gnu)").as_deref(),
            Some("5.9")
        );
        assert_eq!(
            parse_shell_version("fish, version 3.7.1").as_deref(),
            Some("3.7.1")
        );
        assert_eq!(parse_shell_version("nothing here"), None);
        assert_eq!(parse_shell_version(""), None);
    }

    #[test]
    fn load_average_parsing() {
        assert_eq!(
            parse_loadavg("0.52 0.58 0.59 1/456 12345\n"),
            Some([0.52, 0.58, 0.59])
        );
        assert_eq!(parse_loadavg("garbage"), None);
        assert_eq!(parse_loadavg(""), None);
    }

    #[test]
    fn basics_do_not_panic() {
        let _ = (
            load_average(),
            process_count(),
            init_system(),
            locale(),
            shell_with_version(),
        );
        let _ = (kernel(), hostname(), uptime_seconds(), shell(), packages());
        assert!(!detect_os().id.is_empty());
    }
}
