use super::{render_text, Notifier, NotifyError};
use crate::config::{EmailConfig, EmailTransport};
use crate::report::ScanReport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::io::Write;
use std::process::{Command, Stdio};

pub struct EmailNotifier {
    cfg: EmailConfig,
}

fn err(m: String) -> NotifyError {
    NotifyError {
        channel: "email".into(),
        message: m,
    }
}

impl EmailNotifier {
    pub fn new(cfg: EmailConfig) -> Self {
        EmailNotifier { cfg }
    }

    fn subject(&self, report: &ScanReport) -> String {
        format!("SkillShield: {} change(s)", report.findings.len())
    }

    /// Plaintext RFC-822 message piped to `sendmail -t`.
    pub fn build_message(&self, report: &ScanReport) -> String {
        format!(
            "To: {}\r\nFrom: {}\r\nSubject: {}\r\n\r\n{}",
            self.cfg.to,
            self.cfg.from,
            self.subject(report),
            render_text(report)
        )
    }

    /// Structured `lettre` message for the SMTP transport.
    pub fn build_smtp_message(&self, report: &ScanReport) -> Result<Message, NotifyError> {
        Message::builder()
            .from(
                self.cfg
                    .from
                    .parse()
                    .map_err(|e: lettre::address::AddressError| err(e.to_string()))?,
            )
            .to(self
                .cfg
                .to
                .parse()
                .map_err(|e: lettre::address::AddressError| err(e.to_string()))?)
            .subject(self.subject(report))
            .body(render_text(report))
            .map_err(|e| err(e.to_string()))
    }

    fn send_sendmail(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let mut child = Command::new(&self.cfg.sendmail_path)
            .arg("-t")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| err(e.to_string()))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| err("no stdin".into()))?
            .write_all(self.build_message(report).as_bytes())
            .map_err(|e| err(e.to_string()))?;
        let status = child.wait().map_err(|e| err(e.to_string()))?;
        if !status.success() {
            return Err(err(format!("sendmail exited with {status}")));
        }
        Ok(())
    }

    fn send_smtp(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let smtp = self.cfg.smtp.as_ref().ok_or_else(|| {
            err("smtp transport selected but [notify.email.smtp] is missing".into())
        })?;
        let message = self.build_smtp_message(report)?;
        let mut builder = if smtp.starttls {
            SmtpTransport::starttls_relay(&smtp.host).map_err(|e| err(e.to_string()))?
        } else {
            SmtpTransport::relay(&smtp.host).map_err(|e| err(e.to_string()))?
        };
        builder = builder.port(smtp.port);
        if let (Some(u), Some(p)) = (&smtp.username, &smtp.password) {
            builder = builder.credentials(Credentials::new(u.clone(), p.clone()));
        }
        builder
            .build()
            .send(&message)
            .map_err(|e| err(e.to_string()))?;
        Ok(())
    }
}

impl Notifier for EmailNotifier {
    fn id(&self) -> &str {
        "email"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        if !report.has_changes() {
            return Ok(());
        }
        match self.cfg.transport {
            EmailTransport::Sendmail => self.send_sendmail(report),
            EmailTransport::Smtp => self.send_smtp(report),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmailConfig;
    use crate::diff::ScanDiff;
    use crate::report::ScanReport;

    use crate::config::EmailTransport;

    fn base_cfg() -> EmailConfig {
        EmailConfig {
            to: "me@example.com".into(),
            from: "ss@host.example".into(),
            transport: EmailTransport::Sendmail,
            sendmail_path: "/bin/true".into(),
            smtp: None,
        }
    }

    #[test]
    fn builds_rfc822_message_with_headers() {
        let n = EmailNotifier::new(base_cfg());
        let report = ScanReport::from_diff(&ScanDiff { findings: vec![] }, &[], 42);
        let msg = n.build_message(&report);
        assert!(msg.starts_with("To: me@example.com\r\n"));
        assert!(msg.contains("From: ss@host.example\r\n"));
        assert!(msg.contains("Subject: SkillShield"));
    }

    #[test]
    fn builds_lettre_message_for_smtp() {
        let n = EmailNotifier::new(base_cfg());
        let report = ScanReport::from_diff(&ScanDiff { findings: vec![] }, &[], 42);
        // Valid addresses parse into a lettre Message without error.
        assert!(n.build_smtp_message(&report).is_ok());
    }
}
