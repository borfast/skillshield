//! User configuration loaded from the XDG config file, with defaults for every
//! field so the tool runs with no config present.

use crate::error::{Error, Result};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub scan: ScanConfig,
    pub catalog: CatalogConfig,
    pub notify: NotifyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    /// Reserved for future use. Discovery never follows symlinks regardless
    /// of this value — it always scans with `follow_links(false)` per the
    /// never-follow binding constraint — so this field is currently unread.
    pub follow_symlinks: bool,
    pub max_hash_bytes: u64,
    pub project_roots: Vec<String>,
    pub project_max_depth: usize,
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogConfig {
    pub extra_files: Vec<String>,
    pub disable: Vec<String>,
    /// Allowlist of global group keys to monitor (e.g. `["claude.core"]`).
    /// `None` (absent) monitors all groups — `init` writes the user's choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    pub channels: Vec<String>,
    pub email: Option<EmailConfig>,
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmailTransport {
    Sendmail,
    Smtp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub to: String,
    pub from: String,
    #[serde(default = "default_transport")]
    pub transport: EmailTransport,
    #[serde(default = "default_sendmail")]
    pub sendmail_path: String,
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub starttls: bool,
}

fn default_transport() -> EmailTransport {
    EmailTransport::Sendmail
}

fn default_sendmail() -> String {
    "/usr/sbin/sendmail".to_string()
}

fn default_smtp_port() -> u16 {
    587
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig {
            follow_symlinks: false,
            max_hash_bytes: 5_000_000,
            project_roots: Vec::new(),
            project_max_depth: 6,
            ignore: vec![
                "**/node_modules/**".into(),
                "**/.git/**".into(),
                "**/target/**".into(),
                "**/vendor/**".into(),
            ],
        }
    }
}

impl Default for NotifyConfig {
    fn default() -> Self {
        NotifyConfig {
            channels: vec!["report".into(), "stdout".into()],
            email: None,
            webhook: None,
        }
    }
}

impl Config {
    pub fn load_from(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| Error::Serde(e.to_string()))
    }

    pub fn load() -> Result<Config> {
        Self::load_from(&paths::config_path()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert!(!c.scan.follow_symlinks);
        assert_eq!(c.scan.max_hash_bytes, 5_000_000);
        assert!(c.scan.project_roots.is_empty());
        assert_eq!(c.scan.project_max_depth, 6);
        assert_eq!(c.notify.channels, vec!["report", "stdout"]);
    }

    #[test]
    fn absent_file_yields_defaults() {
        let c = Config::load_from(std::path::Path::new("/nonexistent/xyz.toml")).unwrap();
        assert_eq!(c.notify.channels, vec!["report", "stdout"]);
    }

    #[test]
    fn partial_toml_overrides_only_named_fields() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "[scan]\nproject_roots = [\"~/work\"]\n").unwrap();
        let c = Config::load_from(f.path()).unwrap();
        assert_eq!(c.scan.project_roots, vec!["~/work"]);
        // untouched field keeps its default
        assert_eq!(c.scan.max_hash_bytes, 5_000_000);
    }

    #[test]
    fn default_config_serializes_and_round_trips() {
        // The CLI writes `toml::to_string_pretty(&Config::default())` as the
        // initial config file; it must parse back to the same values.
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(
            toml::to_string_pretty(&parsed).unwrap(),
            text,
            "default config must round-trip through TOML"
        );
    }

    #[test]
    fn monitor_selection_round_trips() {
        let mut c = Config::default();
        c.catalog.monitor = Some(vec!["claude.core".into(), "gemini".into()]);
        let text = toml::to_string_pretty(&c).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(
            parsed.catalog.monitor.as_deref(),
            Some(&["claude.core".to_string(), "gemini".to_string()][..])
        );
    }
}
