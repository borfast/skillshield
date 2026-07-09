//! SkillShield core: scan → diff → notify pipeline.

pub mod error;
pub mod paths;
pub mod entry;
pub mod hashing;

pub use error::{Error, Result};
pub use entry::{Entry, EntryKind};
