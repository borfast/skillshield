//! The structured scan report (findings, counts, scan errors, timestamp) that
//! notification channels render.

use crate::diff::{ChangeKind, Finding, ScanDiff};
use crate::discovery::ScanError;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
    pub scan_errors: Vec<ScanError>,
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub generated_at: u64,
}

impl ScanReport {
    pub fn from_diff(diff: &ScanDiff, errors: &[ScanError], now: u64) -> Self {
        let count = |k: ChangeKind| diff.findings.iter().filter(|f| f.change == k).count();
        ScanReport {
            findings: diff.findings.clone(),
            scan_errors: errors.to_vec(),
            added: count(ChangeKind::Added),
            modified: count(ChangeKind::Modified),
            removed: count(ChangeKind::Removed),
            generated_at: now,
        }
    }

    pub fn has_changes(&self) -> bool {
        !self.findings.is_empty()
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
