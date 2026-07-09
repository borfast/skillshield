use crate::error::{Error, Result};
use directories::BaseDirs;
use std::path::{Path, PathBuf};

pub fn home_dir() -> Result<PathBuf> {
    BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or(Error::NoHome)
}

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

fn app_subdir(base: &Path) -> PathBuf {
    base.join("skillshield")
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
}
