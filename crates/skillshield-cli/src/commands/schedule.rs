//! `skillshield schedule` — install or remove a periodic `scan` schedule via a
//! systemd user timer (preferred) or the user crontab (fallback).
//!
//! System-modifying actions print exactly what they will do and prompt for
//! confirmation (unless `--yes`), and are idempotent: re-running replaces the
//! managed unit files / the single marked crontab line rather than duplicating.

use crate::cli::Interval;
use crate::commands::to_err;
use crate::exit::Code;
use std::io::{self, Write};
use std::process::{Command, Stdio};

/// Marker appended to the managed crontab line so it can be found and replaced
/// or removed idempotently.
const CRON_MARKER: &str = "# skillshield-managed";
const TIMER_UNIT: &str = "skillshield.timer";
const SERVICE_UNIT: &str = "skillshield.service";

pub struct Opts {
    pub remove: bool,
    pub force_systemd: bool,
    pub force_cron: bool,
    pub yes: bool,
    pub interval: Interval,
    pub time: String,
}

pub fn run(opts: Opts) -> Result<i32, String> {
    let (hour, minute) = parse_hm(&opts.time)?;
    let exe = std::env::current_exe()
        .map_err(to_err)?
        .to_string_lossy()
        .into_owned();

    let backend = choose_backend(&opts)?;

    if opts.remove {
        remove_schedule(backend, &opts)
    } else {
        install_schedule(backend, &opts, &exe, hour, minute)
    }
}

// ---- backend selection -----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Systemd,
    Cron,
}

fn choose_backend(opts: &Opts) -> Result<Backend, String> {
    if opts.force_systemd {
        return Ok(Backend::Systemd);
    }
    if opts.force_cron {
        return Ok(Backend::Cron);
    }
    if systemd_user_available() {
        Ok(Backend::Systemd)
    } else {
        Ok(Backend::Cron)
    }
}

/// True when a systemd *user* manager is reachable — `systemctl --user
/// show-environment` succeeds only then (it needs the binary on PATH and a
/// running user instance / `XDG_RUNTIME_DIR`).
fn systemd_user_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---- pure artifact generation (unit-tested) --------------------------------

/// Parse "HH:MM" into (hour, minute), validating ranges.
fn parse_hm(s: &str) -> Result<(u32, u32), String> {
    let (h, m) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid --time '{s}', expected HH:MM"))?;
    let hour: u32 = h
        .parse()
        .map_err(|_| format!("invalid hour in --time '{s}'"))?;
    let minute: u32 = m
        .parse()
        .map_err(|_| format!("invalid minute in --time '{s}'"))?;
    if hour > 23 {
        return Err(format!("hour out of range in --time '{s}' (0-23)"));
    }
    if minute > 59 {
        return Err(format!("minute out of range in --time '{s}' (0-59)"));
    }
    Ok((hour, minute))
}

/// systemd `OnCalendar=` expression. Hourly runs at `minute` past every hour;
/// daily runs once at `hour:minute`.
fn oncalendar(interval: Interval, hour: u32, minute: u32) -> String {
    match interval {
        Interval::Hourly => format!("*-*-* *:{minute:02}:00"),
        Interval::Daily => format!("*-*-* {hour:02}:{minute:02}:00"),
    }
}

fn service_unit(exe: &str) -> String {
    format!(
        "[Unit]\n\
         Description=SkillShield scan for AI-agent config changes\n\n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={exe} scan\n\
         # Exit code 10 means \"changes detected\" — a success for the timer.\n\
         SuccessExitStatus=10\n"
    )
}

fn timer_unit(interval: Interval, hour: u32, minute: u32) -> String {
    format!(
        "[Unit]\n\
         Description=Run SkillShield periodically\n\n\
         [Timer]\n\
         OnCalendar={}\n\
         Persistent=true\n\n\
         [Install]\n\
         WantedBy=timers.target\n",
        oncalendar(interval, hour, minute)
    )
}

/// The `min hr dom mon dow` schedule fields for cron.
fn cron_schedule(interval: Interval, hour: u32, minute: u32) -> String {
    match interval {
        Interval::Hourly => format!("{minute} * * * *"),
        Interval::Daily => format!("{minute} {hour} * * *"),
    }
}

/// The full managed crontab line (with trailing marker).
fn cron_line(exe: &str, interval: Interval, hour: u32, minute: u32, log: &str) -> String {
    format!(
        "{} {exe} scan >> {log} 2>&1 {CRON_MARKER}",
        cron_schedule(interval, hour, minute)
    )
}

/// Return `existing` crontab text with any managed line removed, then append
/// `new_line` if given. Idempotent: never leaves more than one managed line.
fn update_crontab(existing: &str, new_line: Option<&str>) -> String {
    let mut lines: Vec<&str> = existing
        .lines()
        .filter(|l| !l.contains(CRON_MARKER))
        .collect();
    if let Some(line) = new_line {
        lines.push(line);
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn cron_log_path() -> String {
    skillshield_core::paths::report_path()
        .map(|p| p.with_file_name("cron.log").to_string_lossy().into_owned())
        .unwrap_or_else(|_| "skillshield-cron.log".to_string())
}

// ---- install / remove orchestration ----------------------------------------

fn confirm(yes: bool, prompt: &str) -> Result<bool, String> {
    if yes {
        return Ok(true);
    }
    print!("{prompt} [y/N] ");
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(to_err)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn install_schedule(
    backend: Backend,
    opts: &Opts,
    exe: &str,
    hour: u32,
    minute: u32,
) -> Result<i32, String> {
    match backend {
        Backend::Systemd => {
            let dir = systemd_user_dir()?;
            let service = service_unit(exe);
            let timer = timer_unit(opts.interval, hour, minute);
            println!(
                "About to install a systemd user timer:\n  {}\n  {}\n\n{SERVICE_UNIT}:\n{service}\n{TIMER_UNIT}:\n{timer}\nThen: systemctl --user daemon-reload && systemctl --user enable --now {TIMER_UNIT}",
                dir.join(SERVICE_UNIT).display(),
                dir.join(TIMER_UNIT).display(),
            );
            if !confirm(opts.yes, "Proceed?")? {
                println!("Aborted. Nothing changed.");
                return Ok(Code::OK);
            }
            std::fs::create_dir_all(&dir).map_err(to_err)?;
            std::fs::write(dir.join(SERVICE_UNIT), service).map_err(to_err)?;
            std::fs::write(dir.join(TIMER_UNIT), timer).map_err(to_err)?;
            systemctl(&["daemon-reload"])?;
            systemctl(&["enable", "--now", TIMER_UNIT])?;
            println!("Installed. Check status: systemctl --user list-timers {TIMER_UNIT}");
            Ok(Code::OK)
        }
        Backend::Cron => {
            let log = cron_log_path();
            let line = cron_line(exe, opts.interval, hour, minute, &log);
            let existing = read_crontab();
            let updated = update_crontab(&existing, Some(&line));
            println!("About to set this crontab line:\n  {line}\n");
            if !confirm(opts.yes, "Proceed?")? {
                println!("Aborted. Nothing changed.");
                return Ok(Code::OK);
            }
            write_crontab(&updated)?;
            println!("Installed. View with: crontab -l");
            Ok(Code::OK)
        }
    }
}

fn remove_schedule(backend: Backend, opts: &Opts) -> Result<i32, String> {
    match backend {
        Backend::Systemd => {
            let dir = systemd_user_dir()?;
            println!("About to disable and remove the systemd user timer ({TIMER_UNIT}, {SERVICE_UNIT}).");
            if !confirm(opts.yes, "Proceed?")? {
                println!("Aborted. Nothing changed.");
                return Ok(Code::OK);
            }
            // Best-effort disable; ignore failure (may already be gone).
            let _ = systemctl(&["disable", "--now", TIMER_UNIT]);
            for unit in [TIMER_UNIT, SERVICE_UNIT] {
                let path = dir.join(unit);
                if path.exists() {
                    std::fs::remove_file(&path).map_err(to_err)?;
                }
            }
            let _ = systemctl(&["daemon-reload"]);
            println!("Removed.");
            Ok(Code::OK)
        }
        Backend::Cron => {
            let existing = read_crontab();
            if !existing.contains(CRON_MARKER) {
                println!("No managed crontab line found; nothing to remove.");
                return Ok(Code::OK);
            }
            println!("About to remove the managed crontab line ({CRON_MARKER}).");
            if !confirm(opts.yes, "Proceed?")? {
                println!("Aborted. Nothing changed.");
                return Ok(Code::OK);
            }
            let updated = update_crontab(&existing, None);
            write_crontab(&updated)?;
            println!("Removed.");
            Ok(Code::OK)
        }
    }
}

// ---- thin system wrappers ---------------------------------------------------

fn systemd_user_dir() -> Result<std::path::PathBuf, String> {
    Ok(skillshield_core::paths::config_dir()
        .map_err(to_err)?
        .join("systemd/user"))
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let mut full = vec!["--user"];
    full.extend_from_slice(args);
    let status = Command::new("systemctl")
        .args(&full)
        .status()
        .map_err(to_err)?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`systemctl {}` failed ({status})", full.join(" ")))
    }
}

fn read_crontab() -> String {
    // `crontab -l` exits non-zero when no crontab exists; treat that as empty.
    Command::new("crontab")
        .arg("-l")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn write_crontab(content: &str) -> Result<(), String> {
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(to_err)?;
    child
        .stdin
        .as_mut()
        .ok_or("no stdin for crontab")?
        .write_all(content.as_bytes())
        .map_err(to_err)?;
    let status = child.wait().map_err(to_err)?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`crontab -` failed ({status})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hm_valid_and_invalid() {
        assert_eq!(parse_hm("09:30").unwrap(), (9, 30));
        assert_eq!(parse_hm("0:0").unwrap(), (0, 0));
        assert!(parse_hm("24:00").is_err());
        assert!(parse_hm("10:60").is_err());
        assert!(parse_hm("nope").is_err());
    }

    #[test]
    fn oncalendar_hourly_uses_minute_only() {
        assert_eq!(oncalendar(Interval::Hourly, 9, 30), "*-*-* *:30:00");
        assert_eq!(oncalendar(Interval::Daily, 9, 5), "*-*-* 09:05:00");
    }

    #[test]
    fn cron_schedule_fields() {
        assert_eq!(cron_schedule(Interval::Hourly, 9, 30), "30 * * * *");
        assert_eq!(cron_schedule(Interval::Daily, 9, 5), "5 9 * * *");
    }

    #[test]
    fn service_unit_has_success_exit_10_and_exe() {
        let s = service_unit("/opt/bin/skillshield");
        assert!(s.contains("ExecStart=/opt/bin/skillshield scan"));
        assert!(s.contains("SuccessExitStatus=10"));
    }

    #[test]
    fn update_crontab_is_idempotent() {
        let line = cron_line("/bin/skillshield", Interval::Hourly, 0, 0, "/log");
        // Insert into an empty crontab.
        let once = update_crontab("", Some(&line));
        assert!(once.contains(CRON_MARKER));
        // Re-running replaces, never duplicates.
        let twice = update_crontab(&once, Some(&line));
        assert_eq!(once, twice);
        assert_eq!(twice.matches(CRON_MARKER).count(), 1);
    }

    #[test]
    fn update_crontab_preserves_other_lines_and_removes_managed() {
        let existing = "0 3 * * * /usr/bin/backup\n";
        let line = cron_line("/bin/skillshield", Interval::Daily, 9, 0, "/log");
        let with = update_crontab(existing, Some(&line));
        assert!(with.contains("/usr/bin/backup"));
        assert!(with.contains(CRON_MARKER));
        // Removing strips only the managed line, keeping the user's own.
        let without = update_crontab(&with, None);
        assert!(without.contains("/usr/bin/backup"));
        assert!(!without.contains(CRON_MARKER));
    }
}
