//! The trusted baseline snapshot: the set of monitored entries plus a
//! self-integrity digest, persisted atomically and verified on load.

use crate::entry::Entry;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct Baseline {
    pub version: u32,
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct OnDisk {
    version: u32,
    integrity: String,
    entries: Vec<Entry>,
}

impl Baseline {
    pub fn new(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Baseline {
            version: CURRENT_VERSION,
            entries,
        }
    }

    pub fn integrity_digest(&self) -> String {
        let json = serde_json::to_vec(&self.entries).unwrap_or_default();
        let mut h = Sha256::new();
        h.update(&json);
        format!("sha256:{:x}", h.finalize())
    }

    pub fn contains_path(&self, p: &Path) -> bool {
        self.entries.iter().any(|e| e.path == p)
    }

    pub fn upsert(&mut self, entry: Entry) {
        match self.entries.binary_search_by(|e| e.path.cmp(&entry.path)) {
            Ok(i) => self.entries[i] = entry,
            Err(i) => self.entries.insert(i, entry),
        }
    }

    pub fn remove_under(&mut self, prefix: &Path) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| !e.path.starts_with(prefix));
        before - self.entries.len()
    }

    pub fn load(path: &Path) -> Result<Baseline> {
        // Read raw bytes so a genuine I/O failure (missing/unreadable file)
        // stays `Io`, while invalid content — non-UTF-8 bytes or bad JSON — is
        // classified as `Corrupt` (tampering/damage), never silently reset.
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::Corrupt("baseline is not valid UTF-8".into()))?;
        let disk: OnDisk =
            serde_json::from_str(&text).map_err(|e| Error::Corrupt(e.to_string()))?;
        if disk.version != CURRENT_VERSION {
            return Err(Error::Corrupt(format!(
                "unsupported baseline version {}",
                disk.version
            )));
        }
        let b = Baseline {
            version: disk.version,
            entries: disk.entries,
        };
        if b.integrity_digest() != disk.integrity {
            return Err(Error::Corrupt("baseline integrity digest mismatch".into()));
        }
        Ok(b)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let disk = OnDisk {
            version: self.version,
            integrity: self.integrity_digest(),
            entries: self.entries.clone(),
        };
        let json = serde_json::to_vec_pretty(&disk).map_err(|e| Error::Serde(e.to_string()))?;

        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(&json)?;
        tmp.flush()?;
        set_owner_only(tmp.path())?;
        tmp.persist(path).map_err(|e| Error::Io(e.error))?;
        Ok(())
    }
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, EntryKind};

    fn e(path: &str, digest: &str) -> Entry {
        Entry {
            path: path.into(),
            kind: EntryKind::File,
            digest: Some(digest.into()),
            symlink_target: None,
            size: 1,
            mtime: 0,
            unhashed: false,
            source_rule: "r".into(),
        }
    }

    #[test]
    fn round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("baseline.json");
        let b = Baseline::new(vec![e("/a", "sha256:1"), e("/b", "sha256:2")]);
        b.save(&path).unwrap();
        let loaded = Baseline::load(&path).unwrap();
        assert_eq!(loaded.entries, b.entries);
        assert_eq!(loaded.version, CURRENT_VERSION);
    }

    #[test]
    fn detects_tampering() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("baseline.json");
        Baseline::new(vec![e("/a", "sha256:1")])
            .save(&path)
            .unwrap();

        // Tamper: flip a digest but leave the stored integrity digest alone.
        let text = std::fs::read_to_string(&path).unwrap();
        let tampered = text.replace("sha256:1", "sha256:evil");
        std::fs::write(&path, tampered).unwrap();

        let err = Baseline::load(&path).unwrap_err();
        assert!(matches!(err, crate::error::Error::Corrupt(_)));
    }

    #[test]
    fn non_utf8_baseline_is_corrupt_not_io() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("baseline.json");
        // Invalid UTF-8 bytes: damaged/tampered content, not an I/O failure.
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x9c]).unwrap();
        let err = Baseline::load(&path).unwrap_err();
        assert!(matches!(err, crate::error::Error::Corrupt(_)));
    }

    #[test]
    fn upsert_and_remove() {
        let mut b = Baseline::new(vec![e("/a", "sha256:1")]);
        b.upsert(e("/a", "sha256:changed"));
        assert_eq!(b.entries.len(), 1);
        assert_eq!(b.entries[0].digest.as_deref(), Some("sha256:changed"));
        b.upsert(e("/proj/x", "sha256:9"));
        assert_eq!(b.remove_under(std::path::Path::new("/proj")), 1);
        assert!(!b.contains_path(std::path::Path::new("/proj/x")));
    }
}
