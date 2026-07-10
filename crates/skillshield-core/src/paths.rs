//! XDG-aware config/data path resolution, `~` expansion, and symlink-free
//! path normalization for matching user input against stored entries.

use crate::error::{Error, Result};
use directories::BaseDirs;
use std::path::{Path, PathBuf};

pub fn home_dir() -> Result<PathBuf> {
    BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or(Error::NoHome)
}

/// Expand a leading `~` / `~/` to the home directory.
///
/// Intentionally infallible: it is called per-rule throughout discovery, and on
/// the only supported platforms (Linux/macOS) `$HOME` is always set, so the
/// `NoHome` fallback (returning the literal `~` path) is unreachable in
/// practice. Threading a `Result` through discovery for that case would not
/// earn its cost; a literal `~` path simply matches nothing, which is safe for
/// a tripwire.
pub fn expand_tilde(s: &str) -> PathBuf {
    if s == "~" {
        return home_dir().unwrap_or_else(|_| PathBuf::from("~"));
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

/// Normalize a user-supplied path to the same form discovery stores paths in,
/// WITHOUT resolving symlinks: expand a leading `~`, make it absolute against
/// the current directory if relative, and lexically clean `.`/`..` components.
///
/// This deliberately does NOT call `std::fs::canonicalize`. Discovery records
/// entries via [`expand_tilde`] (symlinks unresolved), so canonicalizing a
/// path copied from `scan`/`status` output would resolve symlink components in
/// the prefix (e.g. a symlinked `$HOME`) and no longer match the stored entry.
/// Normalizing without resolution keeps an already-absolute path as printed, so
/// `trust`/`monitor` match discovery's entries.
pub fn normalize(path: &Path) -> PathBuf {
    let expanded = match path.to_str() {
        Some(s) => expand_tilde(s),
        None => path.to_path_buf(),
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&expanded))
            .unwrap_or(expanded)
    };
    lexical_clean(&absolute)
}

/// Resolve `.`/`..` components purely lexically (no filesystem access, no
/// symlink resolution). `..` past the root is a no-op.
fn lexical_clean(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn app_subdir(base: &Path) -> PathBuf {
    base.join("skillshield")
}

/// The XDG config base directory (e.g. `~/.config`), not the app subdir.
pub fn config_dir() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(b.config_dir().to_path_buf())
}

pub fn config_path() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(app_subdir(b.config_dir()).join("config.toml"))
}

pub fn baseline_path() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(app_subdir(b.data_dir()).join("baseline.json"))
}

pub fn report_path() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(app_subdir(b.data_dir()).join("last-report.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expands_to_home() {
        let home = home_dir().unwrap();
        assert_eq!(expand_tilde("~/foo"), home.join("foo"));
    }

    #[test]
    fn tilde_bare_is_home() {
        assert_eq!(expand_tilde("~"), home_dir().unwrap());
    }

    #[test]
    fn non_tilde_is_verbatim() {
        assert_eq!(expand_tilde("/etc/foo"), PathBuf::from("/etc/foo"));
    }

    #[test]
    fn config_path_ends_correctly() {
        let p = config_path().unwrap();
        assert!(p.ends_with("skillshield/config.toml"));
    }

    #[test]
    fn normalize_expands_tilde_and_cleans_dotdot() {
        let home = home_dir().unwrap();
        assert_eq!(normalize(std::path::Path::new("~/a/../b")), home.join("b"));
    }

    #[test]
    fn normalize_leaves_absolute_path_as_is() {
        // A path as printed by scan/status output must pass through unchanged.
        assert_eq!(
            normalize(std::path::Path::new("/home/user/.claude/skills/x")),
            PathBuf::from("/home/user/.claude/skills/x")
        );
    }

    #[test]
    fn normalize_does_not_resolve_symlinks() {
        // Unlike std::fs::canonicalize, a symlink component is preserved, so the
        // result still matches how discovery (via expand_tilde) stored the path.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let normalized = normalize(&link.join("file.txt"));
        assert_eq!(normalized, link.join("file.txt"));
        assert_ne!(normalized, real.join("file.txt"));
    }
}
