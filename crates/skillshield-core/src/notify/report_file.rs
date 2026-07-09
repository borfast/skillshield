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

        // Also append a human-readable line-log next to the JSON. This is
        // best-effort by design: a failure to open or write the log must not
        // break the notifier, since the JSON report above is the durable
        // artifact. On unix, create it owner-only (0600) since it contains
        // file paths and digests; this only affects newly-created files.
        let log = path.with_file_name("skillshield.log");
        let mut open_opts = std::fs::OpenOptions::new();
        open_opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open_opts.mode(0o600);
        }
        if let Ok(mut f) = open_opts.open(&log) {
            let _ = writeln!(f, "--- {} ---\n{}", report.generated_at, render_text(report));
        }
        Ok(())
    }
}
