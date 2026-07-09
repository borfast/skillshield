use super::{change_subject, render_text, Notifier, NotifyError};
use crate::report::ScanReport;

pub struct DesktopNotifier;

pub fn graphical_session_available() -> bool {
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

impl Notifier for DesktopNotifier {
    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let err = |m: String| NotifyError {
            channel: "desktop".into(),
            message: m,
        };
        if !report.has_changes() {
            return Ok(());
        }
        notify_rust::Notification::new()
            .summary(&change_subject(report))
            .body(&render_text(report))
            .show()
            .map_err(|e| err(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_graphical_env() {
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        assert!(graphical_session_available());
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        assert!(!graphical_session_available());
    }
}
