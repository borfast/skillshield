use super::{render_text, NotifyError, Notifier};
use crate::paths;
use crate::report::ScanReport;
use std::io::Write;
use std::path::PathBuf;

pub struct ReportFileNotifier {
    pub path: Option<PathBuf>,
}

impl Default for ReportFileNotifier {
    fn default() -> Self {
        ReportFileNotifier { path: paths::report_path().ok() }
    }
}

impl Notifier for ReportFileNotifier {
    fn id(&self) -> &str {
        "report"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let err = |m: String| NotifyError { channel: "report".into(), message: m };
        let path = self.path.clone().ok_or_else(|| err("no report path".into()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| err(e.to_string()))?;
        }
        let json = serde_json::to_vec_pretty(report).map_err(|e| err(e.to_string()))?;
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| err(e.to_string()))?;
        tmp.write_all(&json).map_err(|e| err(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))
                .map_err(|e| err(e.to_string()))?;
        }
        tmp.persist(&path).map_err(|e| err(e.to_string()))?;

        // Also append a human-readable line-log next to the JSON.
        let log = path.with_file_name("skillshield.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            let _ = writeln!(f, "--- {} ---\n{}", report.generated_at, render_text(report));
        }
        Ok(())
    }
}
