use crate::baseline::Baseline;
use crate::discovery::Scan;
use crate::entry::{Entry, EntryKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub path: PathBuf,
    pub change: ChangeKind,
    pub kind: EntryKind,
    pub rule_id: String,
    pub old_digest: Option<String>,
    pub new_digest: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanDiff {
    pub findings: Vec<Finding>,
}

impl ScanDiff {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}

fn changed(old: &Entry, new: &Entry) -> Option<String> {
    if old.kind != new.kind {
        return Some(format!("type changed ({:?} -> {:?})", old.kind, new.kind));
    }
    if old.digest != new.digest {
        return Some(format!(
            "content changed ({} -> {})",
            old.digest.as_deref().unwrap_or("none"),
            new.digest.as_deref().unwrap_or("none")
        ));
    }
    if old.symlink_target != new.symlink_target {
        return Some(format!(
            "symlink target changed ({} -> {})",
            old.symlink_target.as_deref().unwrap_or("none"),
            new.symlink_target.as_deref().unwrap_or("none")
        ));
    }
    if old.unhashed != new.unhashed {
        return Some("unhashed state changed".to_string());
    }
    None
}

pub fn diff(baseline: &Baseline, scan: &Scan) -> ScanDiff {
    let base: BTreeMap<&PathBuf, &Entry> =
        baseline.entries.iter().map(|e| (&e.path, e)).collect();
    let cur: BTreeMap<&PathBuf, &Entry> =
        scan.entries.iter().map(|e| (&e.path, e)).collect();

    let mut findings = Vec::new();

    for (path, new) in &cur {
        match base.get(*path) {
            None => findings.push(Finding {
                path: (*path).clone(),
                change: ChangeKind::Added,
                kind: new.kind,
                rule_id: new.source_rule.clone(),
                old_digest: None,
                new_digest: new.digest.clone(),
                detail: "new file".into(),
            }),
            Some(old) => {
                if let Some(detail) = changed(old, new) {
                    findings.push(Finding {
                        path: (*path).clone(),
                        change: ChangeKind::Modified,
                        kind: new.kind,
                        rule_id: new.source_rule.clone(),
                        old_digest: old.digest.clone(),
                        new_digest: new.digest.clone(),
                        detail,
                    });
                }
            }
        }
    }

    for (path, old) in &base {
        if !cur.contains_key(*path) {
            findings.push(Finding {
                path: (*path).clone(),
                change: ChangeKind::Removed,
                kind: old.kind,
                rule_id: old.source_rule.clone(),
                old_digest: old.digest.clone(),
                new_digest: None,
                detail: "file removed".into(),
            });
        }
    }

    findings.sort_by(|a, b| a.path.cmp(&b.path));
    ScanDiff { findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::Baseline;
    use crate::discovery::Scan;
    use crate::entry::{Entry, EntryKind};

    fn file(path: &str, digest: &str) -> Entry {
        Entry {
            path: path.into(), kind: EntryKind::File, digest: Some(digest.into()),
            symlink_target: None, size: 1, mtime: 0, unhashed: false, source_rule: "r".into(),
        }
    }
    fn scan(entries: Vec<Entry>) -> Scan {
        Scan { entries, errors: vec![] }
    }

    #[test]
    fn detects_added_modified_removed() {
        let base = Baseline::new(vec![file("/a", "sha256:1"), file("/b", "sha256:2")]);
        let cur = scan(vec![file("/a", "sha256:1"), file("/b", "sha256:CHANGED"), file("/c", "sha256:3")]);
        let d = diff(&base, &cur);
        let by = |p: &str| d.findings.iter().find(|f| f.path == std::path::PathBuf::from(p)).unwrap();
        assert_eq!(by("/c").change, ChangeKind::Added);
        assert_eq!(by("/b").change, ChangeKind::Modified);
        // /a unchanged → not reported
        assert!(!d.findings.iter().any(|f| f.path == std::path::PathBuf::from("/a")));
        assert_eq!(d.findings.len(), 2);
    }

    #[test]
    fn detects_removed() {
        let base = Baseline::new(vec![file("/gone", "sha256:1")]);
        let d = diff(&base, &scan(vec![]));
        assert_eq!(d.findings.len(), 1);
        assert_eq!(d.findings[0].change, ChangeKind::Removed);
    }

    #[test]
    fn symlink_retarget_is_modified() {
        let mut old = file("/l", "sha256:x");
        old.kind = EntryKind::Symlink;
        old.digest = Some("sha256:same".into());
        old.symlink_target = Some("a".into());
        let mut new = old.clone();
        new.symlink_target = Some("b".into());
        let d = diff(&Baseline::new(vec![old]), &scan(vec![new]));
        assert_eq!(d.findings.len(), 1);
        assert_eq!(d.findings[0].change, ChangeKind::Modified);
        assert!(d.findings[0].detail.contains("symlink"));
    }

    #[test]
    fn symlink_content_swap_is_modified() {
        // target string unchanged, but resolved content digest differs
        let mut old = file("/l", "sha256:x");
        old.kind = EntryKind::Symlink;
        old.digest = Some("sha256:old".into());
        old.symlink_target = Some("a".into());
        let mut new = old.clone();
        new.digest = Some("sha256:new".into());
        let d = diff(&Baseline::new(vec![old]), &scan(vec![new]));
        assert_eq!(d.findings.len(), 1);
        assert_eq!(d.findings[0].change, ChangeKind::Modified);
        assert!(d.findings[0].detail.contains("content"));
    }

    #[test]
    fn unhashed_change_is_modified() {
        // Same path, same digest, same symlink_target — only `unhashed` differs.
        let mut old = file("/big", "sha256:x");
        old.unhashed = false;
        let mut new = old.clone();
        new.unhashed = true;
        let d = diff(&Baseline::new(vec![old]), &scan(vec![new]));
        assert_eq!(d.findings.len(), 1);
        assert_eq!(d.findings[0].change, ChangeKind::Modified);
        assert!(d.findings[0].detail.contains("unhashed"));
    }

    #[test]
    fn file_to_symlink_flip_is_modified() {
        // same digest, but kind changes File -> Symlink
        let old = file("/p", "sha256:same");
        let mut new = old.clone();
        new.kind = EntryKind::Symlink;
        new.symlink_target = Some("elsewhere".into());
        let d = diff(&Baseline::new(vec![old]), &scan(vec![new]));
        assert_eq!(d.findings.len(), 1);
        assert_eq!(d.findings[0].change, ChangeKind::Modified);
        assert!(d.findings[0].detail.contains("type changed"));
    }
}
