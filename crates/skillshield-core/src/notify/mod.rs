//! Notification channels: a `Notifier` trait, a static registry that builds
//! the enabled channels from config, and a `dispatch` that runs them with
//! per-channel failure isolation.

pub mod desktop;
pub mod email;
pub mod report_file;
pub mod stdout;
pub mod webhook;

use crate::config::NotifyConfig;
use crate::report::ScanReport;

#[derive(Debug, Clone)]
pub struct NotifyError {
    pub channel: String,
    pub message: String,
}

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.channel, self.message)
    }
}

pub trait Notifier {
    fn notify(&self, report: &ScanReport) -> std::result::Result<(), NotifyError>;
}

/// Short one-line subject shared by the desktop and email channels.
pub fn change_subject(report: &ScanReport) -> String {
    format!("SkillShield: {} change(s)", report.findings.len())
}

pub fn render_text(report: &ScanReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "SkillShield: {} change(s) — Added: {}, Modified: {}, Removed: {}\n",
        report.findings.len(),
        report.added,
        report.modified,
        report.removed
    ));
    for f in &report.findings {
        s.push_str(&format!(
            "  {:?}  {}  [{}]  {}\n",
            f.change,
            f.path.display(),
            f.rule_id,
            f.detail
        ));
    }
    if !report.scan_errors.is_empty() {
        s.push_str(&format!("  {} scan error(s):\n", report.scan_errors.len()));
        for e in &report.scan_errors {
            s.push_str(&format!("    ! {} — {}\n", e.path.display(), e.message));
        }
    }
    s
}

pub fn build_notifiers(cfg: &NotifyConfig) -> Vec<Box<dyn Notifier>> {
    let mut out: Vec<Box<dyn Notifier>> = Vec::new();
    for id in &cfg.channels {
        match id.as_str() {
            "report" => out.push(Box::new(report_file::ReportFileNotifier::default())),
            "stdout" => out.push(Box::new(stdout::StdoutNotifier)),
            "desktop" => out.push(Box::new(crate::notify::desktop::DesktopNotifier)),
            "webhook" => {
                if let Some(w) = &cfg.webhook {
                    out.push(Box::new(crate::notify::webhook::WebhookNotifier::new(
                        w.clone(),
                    )));
                } else {
                    eprintln!(
                        "skillshield: 'webhook' channel enabled but [notify.webhook] is missing"
                    );
                }
            }
            "email" => {
                if let Some(e) = &cfg.email {
                    out.push(Box::new(crate::notify::email::EmailNotifier::new(
                        e.clone(),
                    )));
                } else {
                    eprintln!("skillshield: 'email' channel enabled but [notify.email] is missing");
                }
            }
            other => eprintln!("skillshield: unknown notify channel '{other}', skipping"),
        }
    }
    out
}

pub fn dispatch(notifiers: &[Box<dyn Notifier>], report: &ScanReport) -> Vec<NotifyError> {
    let mut errors = Vec::new();
    for n in notifiers {
        if let Err(e) = n.notify(report) {
            errors.push(e);
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ChangeKind, Finding, ScanDiff};
    use crate::entry::EntryKind;
    use crate::report::ScanReport;

    fn sample_report() -> ScanReport {
        let f = Finding {
            path: "/x/CLAUDE.md".into(),
            change: ChangeKind::Added,
            kind: EntryKind::File,
            rule_id: "proj.claude.md".into(),
            old_digest: None,
            new_digest: Some("sha256:ab".into()),
            detail: "new file".into(),
        };
        ScanReport::from_diff(&ScanDiff { findings: vec![f] }, &[], 1000)
    }

    struct Failing;
    impl Notifier for Failing {
        fn notify(&self, _r: &ScanReport) -> std::result::Result<(), NotifyError> {
            Err(NotifyError {
                channel: "failing".into(),
                message: "boom".into(),
            })
        }
    }
    struct Ok;
    impl Notifier for Ok {
        fn notify(&self, _r: &ScanReport) -> std::result::Result<(), NotifyError> {
            std::result::Result::Ok(())
        }
    }

    #[test]
    fn dispatch_isolates_failures() {
        let notifiers: Vec<Box<dyn Notifier>> = vec![Box::new(Failing), Box::new(Ok)];
        let errs = dispatch(&notifiers, &sample_report());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].channel, "failing");
    }

    #[test]
    fn render_text_mentions_counts_and_path() {
        let text = render_text(&sample_report());
        assert!(text.contains("Added: 1"));
        assert!(text.contains("CLAUDE.md"));
    }
}
