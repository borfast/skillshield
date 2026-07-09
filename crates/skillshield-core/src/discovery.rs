use crate::catalog::{Catalog, MatchSpec, Rule, Scope};
use crate::config::ScanConfig;
use crate::entry::{Entry, EntryKind};
use crate::hashing::{hash_file, hash_symlink_target, HashOutcome};
use crate::paths::expand_tilde;
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Scan {
    pub entries: Vec<Entry>,
    pub errors: Vec<ScanError>,
}

struct Collector {
    entries: BTreeMap<PathBuf, Entry>,
    errors: Vec<ScanError>,
    max_hash_bytes: u64,
}

impl Collector {
    fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
        meta.modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Add a single path as an entry. Does NOT follow symlinks.
    fn add_file(&mut self, path: &Path, rule_id: &str) {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                self.errors.push(ScanError { path: path.into(), message: e.to_string() });
                return;
            }
        };
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(path)
                .ok()
                .map(|t| t.to_string_lossy().into_owned());
            // One-hop hash of the resolved regular-file contents (None for
            // directory/special/dangling targets). Never recurses into a dir.
            let out = match hash_symlink_target(path, self.max_hash_bytes) {
                Ok(out) => out,
                Err(e) => {
                    self.errors.push(ScanError { path: path.into(), message: e.to_string() });
                    HashOutcome { digest: None, size: meta.len(), unhashed: false }
                }
            };
            self.entries.insert(path.into(), Entry {
                path: path.into(),
                kind: EntryKind::Symlink,
                digest: out.digest,
                symlink_target: target,
                size: out.size,
                mtime: Self::mtime_secs(&meta),
                unhashed: out.unhashed,
                source_rule: rule_id.into(),
            });
            return;
        }
        if !meta.file_type().is_file() {
            return; // skip fifos/sockets/etc.
        }
        match hash_file(path, self.max_hash_bytes) {
            Ok(out) => {
                self.entries.insert(path.into(), Entry {
                    path: path.into(),
                    kind: EntryKind::File,
                    digest: out.digest,
                    symlink_target: None,
                    size: out.size,
                    mtime: Self::mtime_secs(&meta),
                    unhashed: out.unhashed,
                    source_rule: rule_id.into(),
                });
            }
            Err(e) => self.errors.push(ScanError { path: path.into(), message: e.to_string() }),
        }
    }

    /// Recursively add every file under `dir` (no symlink following).
    fn add_dir_fileset(&mut self, dir: &Path, rule_id: &str) {
        if !dir.exists() {
            return;
        }
        for entry in WalkDir::new(dir).follow_links(false).into_iter() {
            match entry {
                Ok(e) => {
                    let ft = e.file_type();
                    if ft.is_file() || ft.is_symlink() {
                        self.add_file(e.path(), rule_id);
                    }
                }
                Err(err) => {
                    let p = err.path().map(|p| p.to_path_buf()).unwrap_or_default();
                    self.errors.push(ScanError { path: p, message: err.to_string() });
                }
            }
        }
    }
}

pub fn discover(catalog: &Catalog, cfg: &ScanConfig) -> Scan {
    let mut col = Collector {
        entries: BTreeMap::new(),
        errors: Vec::new(),
        max_hash_bytes: cfg.max_hash_bytes,
    };

    // Global rules: resolve directly.
    for rule in catalog.rules.iter().filter(|r| r.scope == Scope::Global) {
        match &rule.spec {
            MatchSpec::ExactPath(p) => {
                let path = expand_tilde(p);
                if path.exists() {
                    col.add_file(&path, &rule.id);
                }
            }
            MatchSpec::DirFileSet(d) => {
                col.add_dir_fileset(&expand_tilde(d), &rule.id);
            }
            MatchSpec::Glob(g) => {
                // A global glob: match against files in the glob's base directory.
                let expanded = expand_tilde(g);
                if let Some(parent) = expanded.parent() {
                    if let Ok(set) = build_globset(&[expanded.to_string_lossy().into_owned()]) {
                        for e in WalkDir::new(parent).max_depth(1).follow_links(false) {
                            match e {
                                Ok(e) => {
                                    if (e.file_type().is_file() || e.file_type().is_symlink())
                                        && set.is_match(e.path())
                                    {
                                        col.add_file(e.path(), &rule.id);
                                    }
                                }
                                Err(err) => {
                                    let p =
                                        err.path().map(|p| p.to_path_buf()).unwrap_or_default();
                                    col.errors
                                        .push(ScanError { path: p, message: err.to_string() });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Project rules: crawl each opted-in root once, match project rules per file.
    let project_rules: Vec<&Rule> =
        catalog.rules.iter().filter(|r| r.scope == Scope::Project).collect();
    if !project_rules.is_empty() {
        let ignore = build_globset(&cfg.ignore).ok();
        for root in &cfg.project_roots {
            let root_path = expand_tilde(root);
            if !root_path.exists() {
                continue;
            }
            for e in WalkDir::new(&root_path)
                .max_depth(cfg.project_max_depth)
                .follow_links(false)
                .into_iter()
            {
                let e = match e {
                    Ok(e) => e,
                    Err(err) => {
                        let p = err.path().map(|p| p.to_path_buf()).unwrap_or_default();
                        col.errors.push(ScanError { path: p, message: err.to_string() });
                        continue;
                    }
                };
                if let Some(ig) = &ignore {
                    if ig.is_match(e.path()) {
                        continue;
                    }
                }
                if !(e.file_type().is_file() || e.file_type().is_symlink()) {
                    continue;
                }
                let rel = e.path().strip_prefix(&root_path).unwrap_or(e.path());
                for rule in &project_rules {
                    if project_rule_matches(rule, rel, e.path()) {
                        col.add_file(e.path(), &rule.id);
                        break;
                    }
                }
            }
        }
    }

    let entries: Vec<Entry> = col.entries.into_values().collect();
    Scan { entries, errors: col.errors }
}

fn project_rule_matches(rule: &Rule, rel: &Path, _abs: &Path) -> bool {
    match &rule.spec {
        MatchSpec::Glob(g) => single_glob_match(g, rel),
        MatchSpec::DirFileSet(g) => {
            let inside = format!("{}**", g); // "**/.claude/" -> "**/.claude/**"
            single_glob_match(g, rel) || single_glob_match(&inside, rel)
        }
        MatchSpec::ExactPath(_) => false,
    }
}

fn single_glob_match(pattern: &str, rel: &Path) -> bool {
    build_globset(std::slice::from_ref(&pattern.to_string()))
        .map(|s| s.is_match(rel))
        .unwrap_or(false)
}

fn build_globset(patterns: &[String]) -> Result<globset::GlobSet, globset::Error> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p)?);
    }
    b.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, MatchSpec, Rule, Scope};
    use crate::config::ScanConfig;
    use std::fs;

    fn write(p: &std::path::Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn discovers_exact_and_dirfileset_globals() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("mem/CLAUDE.md"), "hi");
        write(&root.join("skills/a/SKILL.md"), "one");
        write(&root.join("skills/b/SKILL.md"), "two");

        let cat = Catalog {
            rules: vec![
                Rule {
                    id: "md".into(),
                    description: "".into(),
                    spec: MatchSpec::ExactPath(root.join("mem/CLAUDE.md").to_string_lossy().into()),
                    scope: Scope::Global,
                },
                Rule {
                    id: "skills".into(),
                    description: "".into(),
                    spec: MatchSpec::DirFileSet(root.join("skills/").to_string_lossy().into()),
                    scope: Scope::Global,
                },
            ],
        };
        let scan = discover(&cat, &ScanConfig::default());
        let paths: Vec<_> = scan.entries.iter().map(|e| e.path.clone()).collect();
        assert!(paths.contains(&root.join("mem/CLAUDE.md")));
        assert!(paths.contains(&root.join("skills/a/SKILL.md")));
        assert!(paths.contains(&root.join("skills/b/SKILL.md")));
        // all hashed
        assert!(scan.entries.iter().all(|e| e.digest.is_some()));
    }

    #[test]
    fn records_symlink_target_and_hashes_resolved_content() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("d/real.md"), "x");
        std::os::unix::fs::symlink("real.md", root.join("d/link.md")).unwrap();

        let cat = Catalog {
            rules: vec![Rule {
                id: "d".into(),
                description: "".into(),
                spec: MatchSpec::DirFileSet(root.join("d/").to_string_lossy().into()),
                scope: Scope::Global,
            }],
        };
        let scan = discover(&cat, &ScanConfig::default());
        let link = scan.entries.iter().find(|e| e.path.ends_with("link.md")).unwrap();
        assert_eq!(link.kind, EntryKind::Symlink);
        assert_eq!(link.symlink_target.as_deref(), Some("real.md"));
        // one-hop content hash of "real.md" (contents "x") is present
        assert!(link.digest.is_some());
    }

    #[test]
    fn project_glob_matches_under_roots_with_ignore() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("proj/AGENTS.md"), "a");
        write(&root.join("proj/node_modules/AGENTS.md"), "ignored");

        let cat = Catalog {
            rules: vec![Rule {
                id: "agents".into(),
                description: "".into(),
                spec: MatchSpec::Glob("**/AGENTS.md".into()),
                scope: Scope::Project,
            }],
        };
        let cfg = ScanConfig {
            project_roots: vec![root.join("proj").to_string_lossy().into()],
            ..ScanConfig::default()
        };
        let scan = discover(&cat, &cfg);
        let paths: Vec<_> = scan.entries.iter().map(|e| e.path.clone()).collect();
        assert!(paths.contains(&root.join("proj/AGENTS.md")));
        assert!(!paths.iter().any(|p| p.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn project_dirfileset_matches_files_inside_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join(".claude/skills/a.md"), "skill");
        write(&root.join(".claude/settings.json"), "{}");

        let cat = Catalog {
            rules: vec![Rule {
                id: "claude-dir".into(),
                description: "".into(),
                spec: MatchSpec::DirFileSet("**/.claude/".into()),
                scope: Scope::Project,
            }],
        };
        let cfg = ScanConfig {
            project_roots: vec![root.to_string_lossy().into()],
            ..ScanConfig::default()
        };
        let scan = discover(&cat, &cfg);
        let paths: Vec<_> = scan.entries.iter().map(|e| e.path.clone()).collect();
        assert!(paths.contains(&root.join(".claude/skills/a.md")));
        assert!(paths.contains(&root.join(".claude/settings.json")));
    }
}
