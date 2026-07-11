use crate::cli::Command;
use skillshield_core::baseline::Baseline;
use skillshield_core::discovery::Scan;
use std::path::Path;

pub mod init;
pub mod monitor;
pub mod review;
pub mod scan;
pub mod schedule;
pub mod status;
pub mod trust;

pub fn run(command: Command) -> Result<i32, String> {
    match command {
        Command::Init { force } => init::run(force),
        Command::Scan { verbose } => scan::run(verbose),
        Command::Status => status::run(),
        Command::Review => review::run(),
        Command::Trust { path } => trust::run(&path),
        Command::Monitor { path } => monitor::run(&path),
        Command::Forget { path } => monitor::run_forget(&path),
        Command::Schedule {
            remove,
            systemd,
            cron,
            yes,
            interval,
            time,
        } => schedule::run(schedule::Opts {
            remove,
            force_systemd: systemd,
            force_cron: cron,
            yes,
            interval,
            time,
        }),
    }
}

// helper reused by several commands
pub fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Normalize a user-supplied path to the form discovery stores entries in,
/// without resolving symlinks (see `skillshield_core::paths::normalize`). Using
/// canonicalize here would resolve symlink prefixes (e.g. a symlinked `$HOME`)
/// and fail to match the paths `scan`/`status` print for global-rule entries.
pub fn abs(path: &Path) -> std::path::PathBuf {
    skillshield_core::paths::normalize(path)
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

pub fn discover_now() -> Result<
    (
        skillshield_core::discovery::Scan,
        skillshield_core::config::Config,
    ),
    String,
> {
    let cfg = skillshield_core::config::Config::load().map_err(to_err)?;
    let catalog = skillshield_core::catalog::Catalog::builtin()
        .apply(&cfg.catalog.disable, &cfg.catalog.extra_files);
    Ok((
        skillshield_core::discovery::discover(&catalog, &cfg.scan),
        cfg,
    ))
}

pub fn save_baseline(baseline: &Baseline) -> Result<(), String> {
    let path = skillshield_core::paths::baseline_path().map_err(to_err)?;
    baseline.save(&path).map_err(to_err)
}

/// Atomically write the config (0600) to `paths::config_path()`.
pub fn write_config(cfg: &skillshield_core::config::Config) -> Result<(), String> {
    use std::io::Write;
    let path = skillshield_core::paths::config_path().map_err(to_err)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(to_err)?;
    }
    let text = toml::to_string_pretty(cfg).map_err(to_err)?;
    let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(to_err)?;
    tmp.write_all(text.as_bytes()).map_err(to_err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))
            .map_err(to_err)?;
    }
    tmp.persist(&path).map_err(|e| to_err(e.error))?;
    Ok(())
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
            path: path.into(),
            kind: EntryKind::File,
            digest: Some(digest.into()),
            symlink_target: None,
            size: 1,
            mtime: 0,
            unhashed: false,
            source_rule: "r".into(),
        }
    }

    #[test]
    fn apply_added_upserts() {
        let mut b = Baseline::new(vec![]);
        let scan = Scan {
            entries: vec![entry("/x", "sha256:1")],
            errors: vec![],
        };
        let changed = apply_finding(&mut b, &scan, std::path::Path::new("/x"));
        assert!(changed);
        assert!(b.contains_path(std::path::Path::new("/x")));
    }

    #[test]
    fn apply_removed_deletes() {
        let mut b = Baseline::new(vec![entry("/gone", "sha256:1")]);
        let scan = Scan {
            entries: vec![],
            errors: vec![],
        };
        let changed = apply_finding(&mut b, &scan, std::path::Path::new("/gone"));
        assert!(changed);
        assert!(!b.contains_path(std::path::Path::new("/gone")));
    }
}
