use crate::cli::Command;
use std::path::Path;

pub mod init;
pub mod scan;
pub mod status;
pub mod review;
pub mod trust;
pub mod monitor;

pub fn run(command: Command) -> Result<i32, String> {
    match command {
        Command::Init { force } => init::run(force),
        Command::Scan => scan::run(),
        Command::Status => status::run(),
        Command::Review => review::run(),
        Command::Trust { path } => trust::run(&path),
        Command::Monitor { path } => monitor::run(&path),
        Command::Unmonitor { path } => monitor::run_unmonitor(&path),
    }
}

// helper reused by several commands
pub fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[allow(dead_code)]
pub fn abs(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn load_baseline_or_hint() -> Result<skillshield_core::baseline::Baseline, String> {
    let path = skillshield_core::paths::baseline_path().map_err(to_err)?;
    if !path.exists() {
        return Err(format!(
            "no baseline at {}. Run `skillshield init` first.",
            path.display()
        ));
    }
    skillshield_core::baseline::Baseline::load(&path).map_err(to_err)
}

pub fn discover_now(
) -> Result<(skillshield_core::discovery::Scan, skillshield_core::config::Config), String> {
    let cfg = skillshield_core::config::Config::load().map_err(to_err)?;
    let catalog = skillshield_core::catalog::Catalog::builtin()
        .apply(&cfg.catalog.disable, &cfg.catalog.extra_files);
    Ok((skillshield_core::discovery::discover(&catalog, &cfg.scan), cfg))
}
