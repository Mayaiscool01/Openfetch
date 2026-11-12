//! End-to-end tests that run the real `openfetch` binary.

use std::path::PathBuf;
use std::process::{Command, Output};

fn home() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openfetch-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_openfetch"))
        .args(args)
        .env("HOME", home())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color")
        .env("LANG", "en_US.UTF-8")
        .output()
        .expect("binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn write_config(name: &str, text: &str) -> String {
    let path = home().join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn version_and_help() {
    let v = run(&["--version"]);
    assert!(v.status.success());
    assert!(stdout(&v).starts_with("openfetch "));
    let h = stdout(&run(&["--help"]));
    for flag in ["--compact", "--json", "--logo", "--animation", "--config"] {
        assert!(h.contains(flag), "help should mention {flag}");
    }
}

#[test]
fn unknown_flag_is_a_usage_error() {
    let o = run(&["--definitely-not-a-flag"]);
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn default_output_is_plain_text_when_piped() {
    let o = run(&[]);
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(!out.contains('\u{1b}'), "no ANSI escapes when piped");
    assert!(out.contains("Software"), "{out}");
    assert!(out.contains("Hardware"));
    assert!(
        out.contains("╭") && out.contains("→"),
        "Unicode panels on a UTF-8 locale"
    );
    assert!(out.contains("GiB"), "memory/disk values are present");
}

#[test]
fn flat_frame_keeps_labels() {
    let cfg = write_config("flat.toml", "[layout]\nframe = \"flat\"\n");
    let out = stdout(&run(&["--config", &cfg]));
    assert!(
        out.contains("OS:") && out.contains("Kernel:") && out.contains("Memory:"),
        "{out}"
    );
    assert!(!out.contains("Software"));
}

#[test]
fn section_titles_and_modules_are_configurable() {
    let cfg = write_config(
        "sections.toml",
        r#"[[sections]]
           title = "My Box"
           modules = ["kernel"]
           bullet = "*"
           [[sections]]
           title = "Other"
           modules = ["kernel", { label = "Note", text = "hello" }]
           labels = true
           [layout]
           logo_position = "none"
        "#,
    );
    let out = stdout(&run(&["--config", &cfg]));
    assert!(out.contains("╭─ My Box ─"), "{out}");
    let kernel = if cfg!(target_os = "macos") {
        "Darwin"
    } else {
        "Linux"
    };
    assert!(out.contains(&format!("│ * {kernel}")), "{out}");
    assert!(out.contains("╭─ Other ─"));
    assert!(
        out.contains("Note:") && out.contains("hello"),
        "labels = true on one section only: {out}"
    );
    assert!(out.contains(&format!("→ Kernel: {kernel}")), "{out}");
    assert!(!out.contains("Software"));
}

#[test]
fn arch_logo_is_solid_block_art_on_unicode_and_ascii_otherwise() {
    let block = stdout(&run(&["--logo", "arch"]));
    assert!(block.contains('█'), "{block}");
    let ascii = stdout(&run(&["--logo", "arch", "--ascii"]));
    assert!(ascii.is_ascii() && ascii.contains("/\\"));
    let cfg = write_config("logo-ascii.toml", "[logo]\nstyle = \"ascii\"\n");
    assert!(stdout(&run(&["--config", &cfg, "--logo", "arch"])).is_ascii());
}

#[test]
fn compact_is_shorter_than_detailed() {
    let full = stdout(&run(&[])).lines().count();
    let compact = stdout(&run(&["--compact"])).lines().count();
    assert!(compact < full, "{compact} < {full}");
}

#[test]
fn json_is_valid_and_complete() {
    let o = run(&["--json"]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("valid JSON");
    for key in [
        "os", "kernel", "cpu", "memory", "disks", "gpus", "network", "firmware",
    ] {
        assert!(v.get(key).is_some(), "missing key {key}");
    }
    assert!(v["memory"]["total"].as_u64().unwrap() > 0);
    assert!(v["cpu"]["threads"].as_u64().unwrap() >= 1);
}

#[test]
fn logo_only_and_named_logos() {
    let auto = stdout(&run(&["--logo"]));
    assert!(auto.lines().count() >= 6);
    assert!(!auto.contains("OS:"));
    assert!(stdout(&run(&["--logo", "arch", "--ascii"])).contains("/\\"));
    let bad = run(&["--logo", "nonexistent"]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("available:"));
}

#[test]
fn lists() {
    let logos = stdout(&run(&["--list-logos"]));
    for id in [
        "fedora", "arch", "debian", "ubuntu", "opensuse", "macos", "linux",
    ] {
        assert!(logos.contains(id));
    }
    assert!(stdout(&run(&["--list-themes"])).contains("nord"));
}

#[test]
fn ascii_flag_gives_pure_ascii() {
    let out = stdout(&run(&["--ascii"]));
    assert!(out.is_ascii(), "{out}");
}

#[test]
fn colour_can_be_forced_on_a_pipe() {
    let cfg = write_config("color.toml", "[general]\ncolor = \"always\"\n");
    let out = stdout(&run(&["--config", &cfg, "--compact"]));
    assert!(out.contains('\u{1b}'));
    let off = stdout(&run(&["--config", &cfg, "--compact", "--no-color"]));
    assert!(!off.contains('\u{1b}'));
}

#[test]
fn config_controls_modules_order_labels_and_custom_rows() {
    let cfg = write_config(
        "mods.toml",
        r#"modules = ["kernel", { label = "Motto", text = "hello" }, "os", "bogus"]
           [labels]
           os = "Distro"
           [layout]
           frame = "flat"
           logo_position = "none"
           key_align = false
           separator = " ="
        "#,
    );
    let o = run(&["--config", &cfg]);
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert!(lines[0].starts_with("Kernel ="), "{lines:?}");
    assert_eq!(lines[1], "Motto = hello");
    assert!(lines[2].starts_with("Distro ="));
    assert!(String::from_utf8_lossy(&o.stderr).contains("unknown module 'bogus'"));
}

#[test]
fn explicit_config_must_exist_and_parse() {
    assert_eq!(
        run(&["--config", "/no/such/file.toml"]).status.code(),
        Some(1)
    );
    let bad = write_config("bad.toml", "[layout\nstyle = ");
    assert_eq!(run(&["--config", &bad]).status.code(), Some(1));
}

#[test]
fn config_without_value_shows_effective_configuration() {
    let out = stdout(&run(&["--config"]));
    assert!(out.contains("Effective configuration"));
    assert!(out.contains("[layout]"));
}

#[test]
fn init_config_writes_once() {
    let target = home().join("init-test").join("config.toml");
    let _ = std::fs::remove_dir_all(target.parent().unwrap());
    let arg = target.to_string_lossy().into_owned();
    assert!(run(&["--init-config", "--config", &arg]).status.success());
    let written = std::fs::read_to_string(&target).unwrap();
    assert!(written.contains("[animation]"));
    assert_eq!(
        run(&["--init-config", "--config", &arg]).status.code(),
        Some(1)
    );
    // and the generated file is itself loadable
    assert!(run(&["--config", &arg, "--compact"]).status.success());
}

#[test]
fn unknown_theme_falls_back_instead_of_failing() {
    let o = run(&["--theme", "nope"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("unknown theme"));
}

#[test]
fn animation_is_skipped_when_not_a_terminal() {
    let out = run(&["--animation", "wave"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Software"));
}

#[test]
fn narrow_terminals_hide_the_logo_and_truncate() {
    let o = Command::new(env!("CARGO_BIN_EXE_openfetch"))
        .args(["--ascii"])
        .env("HOME", home())
        .env("COLUMNS", "40")
        .env("LANG", "C")
        .output()
        .unwrap();
    let out = stdout(&o);
    assert!(out.lines().all(|l| l.chars().count() <= 40), "{out}");
    assert!(
        !out.contains("ssssss") && !out.contains('█'),
        "logo should be hidden"
    );
}

fn png(name: &str) -> String {
    // 2x2 opaque red PNG
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd,
        0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x5e, 0x63, 0xfc,
        0xcf, 0x00, 0x02, 0x4c, 0x60, 0x92, 0x01, 0x00, 0x0d, 0x1d, 0x01, 0x03, 0xae, 0x33, 0x4b,
        0xa8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let path = home().join(name);
    std::fs::write(&path, PNG).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn image_can_be_used_as_the_logo() {
    let img = png("logo.png");
    let o = run(&["--image", &img, "--logo"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = stdout(&o);
    assert!(out.contains('█'), "{out}");
    assert!(out.lines().count() >= 4);
    // from the config file, beside the info
    let cfg = write_config(
        "image.toml",
        &format!("[logo]\nimage = \"{img}\"\nimage_width = 12\n"),
    );
    let out = stdout(&run(&["--config", &cfg]));
    assert!(out.contains('█') && out.contains("Software"), "{out}");
}

#[test]
fn missing_image_is_an_error_on_the_command_line_but_not_in_config() {
    let o = run(&["--image", "/no/such/picture.png"]);
    assert_eq!(o.status.code(), Some(1));
    let cfg = write_config(
        "badimage.toml",
        "[logo]\nimage = \"/no/such/picture.png\"\n",
    );
    let o = run(&["--config", &cfg]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("using the normal logo"));
    assert!(stdout(&o).contains("Software"));
}

#[test]
fn tui_refuses_to_run_without_a_terminal() {
    let o = run(&["--tui"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("interactive terminal"));
}

/// Every IPv4-looking token in `text`.
fn ipv4s(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|t| {
            let parts: Vec<&str> = t.split('.').collect();
            parts.len() == 4
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn noip_hides_addresses_everywhere() {
    for args in [
        vec!["--noip"],
        vec!["--no-ip"],
        vec!["--noip", "--compact"],
        vec!["--noip", "--json"],
    ] {
        let o = run(&args);
        assert!(o.status.success());
        let out = stdout(&o);
        assert!(ipv4s(&out).is_empty(), "{args:?} leaked {:?}", ipv4s(&out));
        assert!(
            !out.contains("local ip") && !out.contains("Local IP"),
            "{args:?}"
        );
    }
    let v: serde_json::Value = serde_json::from_slice(&run(&["--noip", "--json"]).stdout).unwrap();
    assert!(v["network"].is_null());
    // the flat layout too, and from the config file
    let cfg = write_config(
        "noip.toml",
        "[general]\nhide_ip = true\n[layout]\nframe = \"flat\"\n",
    );
    let out = stdout(&run(&["--config", &cfg]));
    assert!(ipv4s(&out).is_empty() && !out.contains("Network"), "{out}");
    assert!(out.contains("Kernel:"), "everything else is still shown");
}
