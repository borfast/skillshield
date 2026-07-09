use crate::cli::Command;
use skillshield_core::baseline::Baseline;
use skillshield_core::discovery::Scan;
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

pub fn save_baseline(baseline: &Baseline) -> Result<(), String> {
    let path = skillshield_core::paths::baseline_path().map_err(to_err)?;
    baseline.save(&path).map_err(to_err)
}

/// Reconcile the baseline with the current scan for one path.
/// Returns true if the baseline changed.
pub fn apply_finding(baseline: &mut Baseline, scan: &Scan, path: &Path) -> bool {
    if let Some(entry) = scan.entries.iter().find(|e| e.path == path) {
        baseline.upsert(entry.clone());
        true
    } else if baseline.contains_path(path) {
        baseline.remove_under(path) > 0
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillshield_core::baseline::Baseline;
    use skillshield_core::discovery::Scan;
    use skillshield_core::entry::{Entry, EntryKind};

    fn entry(path: &str, digest: &str) -> Entry {
        Entry {
            path: path.into(), kind: EntryKind::File, digest: Some(digest.into()),
            symlink_target: None, size: 1, mtime: 0, unhashed: false, source_rule: "r".into(),
        }
    }

    #[test]
    fn apply_added_upserts() {
        let mut b = Baseline::new(vec![]);
        let scan = Scan { entries: vec![entry("/x", "sha256:1")], errors: vec![] };
        let changed = apply_finding(&mut b, &scan, std::path::Path::new("/x"));
        assert!(changed);
        assert!(b.contains_path(std::path::Path::new("/x")));
    }

    #[test]
    fn apply_removed_deletes() {
        let mut b = Baseline::new(vec![entry("/gone", "sha256:1")]);
        let scan = Scan { entries: vec![], errors: vec![] };
        let changed = apply_finding(&mut b, &scan, std::path::Path::new("/gone"));
        assert!(changed);
        assert!(!b.contains_path(std::path::Path::new("/gone")));
    }
}
