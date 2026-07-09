use super::{render_text, Notifier, NotifyError};
use crate::report::ScanReport;

pub struct StdoutNotifier;

impl Notifier for StdoutNotifier {
    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        if report.has_changes() || !report.scan_errors.is_empty() {
            print!("{}", render_text(report));
        } else {
            println!("SkillShield: no changes.");
        }
        Ok(())
    }
}
