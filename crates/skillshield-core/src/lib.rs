//! SkillShield core: scan → diff → notify pipeline.

pub mod error;
pub mod paths;
pub mod entry;
pub mod hashing;
pub mod catalog;
pub mod config;
pub mod discovery;

pub use error::{Error, Result};
pub use entry::{Entry, EntryKind};
pub use config::Config;
pub use discovery::{discover, Scan, ScanError};
