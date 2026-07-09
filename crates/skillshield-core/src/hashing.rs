//! Streaming SHA-256 hashing for regular files and one-hop symlink targets,
//! with a configurable size guard that flags oversized files as unhashed.

use crate::error::Result;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct HashOutcome {
    pub digest: Option<String>,
    pub size: u64,
    pub unhashed: bool,
}

/// Stream a file's bytes through SHA-256. `File::open` follows symlinks, so this
/// reads resolved contents when `path` is a symlink.
fn stream_digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub fn hash_file(path: &Path, max_bytes: u64) -> Result<HashOutcome> {
    let meta = std::fs::symlink_metadata(path)?;
    let size = meta.len();
    if size > max_bytes {
        return Ok(HashOutcome {
            digest: None,
            size,
            unhashed: true,
        });
    }
    Ok(HashOutcome {
        digest: Some(stream_digest(path)?),
        size,
        unhashed: false,
    })
}

/// Hash the regular-file contents a symlink resolves to (one dereference via
/// `std::fs::metadata`, which follows a chain to the final target). Returns
/// `digest = None` for a dangling target, a directory, or a special file.
pub fn hash_symlink_target(link_path: &Path, max_bytes: u64) -> Result<HashOutcome> {
    let meta = match std::fs::metadata(link_path) {
        Ok(m) => m,
        // Dangling or unreadable target: record target string only (no digest).
        Err(_) => {
            return Ok(HashOutcome {
                digest: None,
                size: 0,
                unhashed: false,
            })
        }
    };
    if !meta.file_type().is_file() {
        // Symlink to a directory or special file: do not traverse/hash.
        return Ok(HashOutcome {
            digest: None,
            size: meta.len(),
            unhashed: false,
        });
    }
    let size = meta.len();
    if size > max_bytes {
        return Ok(HashOutcome {
            digest: None,
            size,
            unhashed: true,
        });
    }
    Ok(HashOutcome {
        digest: Some(stream_digest(link_path)?),
        size,
        unhashed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hashes_small_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"hello").unwrap();
        let out = hash_file(f.path(), 1_000_000).unwrap();
        // sha256("hello")
        assert_eq!(
            out.digest.as_deref(),
            Some("sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
        assert_eq!(out.size, 5);
        assert!(!out.unhashed);
    }

    #[test]
    fn oversized_file_is_unhashed_not_skipped() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"0123456789").unwrap();
        let out = hash_file(f.path(), 4).unwrap();
        assert!(out.digest.is_none());
        assert!(out.unhashed);
        assert_eq!(out.size, 10);
    }

    #[test]
    fn symlink_target_content_is_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, b"hello").unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let out = hash_symlink_target(&link, 1_000_000).unwrap();
        assert_eq!(
            out.digest.as_deref(),
            Some("sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    #[test]
    fn dangling_symlink_has_no_digest() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(dir.path().join("does-not-exist"), &link).unwrap();
        let out = hash_symlink_target(&link, 1_000_000).unwrap();
        assert!(out.digest.is_none());
        assert!(!out.unhashed);
    }

    #[test]
    fn symlink_to_directory_has_no_digest() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&subdir, &link).unwrap();

        let out = hash_symlink_target(&link, 1_000_000).unwrap();
        assert!(out.digest.is_none());
        assert!(!out.unhashed);
    }
}
