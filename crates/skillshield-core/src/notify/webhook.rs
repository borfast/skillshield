use super::{Notifier, NotifyError};
use crate::config::WebhookConfig;
use crate::report::ScanReport;

pub struct WebhookNotifier {
    cfg: WebhookConfig,
}

impl WebhookNotifier {
    pub fn new(cfg: WebhookConfig) -> Self {
        WebhookNotifier { cfg }
    }
}

impl Notifier for WebhookNotifier {
    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        // Alert channel: stay quiet on a clean run so a periodic scan doesn't
        // POST "nothing changed" every time (matches desktop/email).
        if !report.has_changes() {
            return Ok(());
        }
        let err = |m: String| NotifyError {
            channel: "webhook".into(),
            message: m,
        };
        let mut req = ureq::post(&self.cfg.url);
        for (k, v) in &self.cfg.headers {
            req = req.set(k, v);
        }
        req.send_json(serde_json::to_value(report).map_err(|e| err(e.to_string()))?)
            .map_err(|e| err(e.to_string()))?;
        Ok(())
    }
}
