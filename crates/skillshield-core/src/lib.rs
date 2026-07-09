//! SkillShield core: scan → diff → notify pipeline.

pub mod baseline;
pub mod catalog;
pub mod config;
pub mod diff;
pub mod discovery;
pub mod entry;
pub mod error;
pub mod hashing;
pub mod notify;
pub mod paths;
pub mod report;

pub use baseline::Baseline;
pub use config::Config;
pub use diff::{diff, ChangeKind, Finding, ScanDiff};
pub use discovery::{discover, Scan, ScanError};
pub use entry::{Entry, EntryKind};
pub use error::{Error, Result};
pub use report::ScanReport;
