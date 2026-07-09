use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_skillshield")
}

#[test]
fn init_then_scan_detects_change() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    // Seed a fake global artifact under a fake HOME.
    std::fs::create_dir_all(home.join(".claude/skills/a")).unwrap();
    std::fs::write(home.join(".claude/skills/a/SKILL.md"), "one").unwrap();

    let config_home = home.join(".config");
    let data_home = home.join(".local/share");
    let envs = [
        ("HOME", home.to_str().unwrap()),
        ("XDG_CONFIG_HOME", config_home.to_str().unwrap()),
        ("XDG_DATA_HOME", data_home.to_str().unwrap()),
    ];

    // init (auto-trust via piped "y")
    let mut init = Command::new(bin())
        .arg("init")
        .envs(envs)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    use std::io::Write;
    init.stdin.as_mut().unwrap().write_all(b"y\n").unwrap();
    assert!(init.wait().unwrap().success());

    // scan: no changes yet → exit 0
    let status = Command::new(bin()).arg("scan").envs(envs).status().unwrap();
    assert_eq!(status.code(), Some(0));

    // introduce a new file → exit 10
    std::fs::write(home.join(".claude/skills/a/EVIL.md"), "payload").unwrap();
    let status = Command::new(bin()).arg("scan").envs(envs).status().unwrap();
    assert_eq!(status.code(), Some(10));
}
