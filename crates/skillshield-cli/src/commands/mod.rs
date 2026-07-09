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
#[allow(dead_code)]
pub fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[allow(dead_code)]
pub fn abs(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
