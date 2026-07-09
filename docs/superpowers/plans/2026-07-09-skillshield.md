# SkillShield Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Linux CLI tripwire that baselines the files AI coding agents consume (skills, plugins, `CLAUDE.md`/`AGENTS.md`, MCP configs, etc.) and warns on any change vs. that baseline.

**Architecture:** A Cargo workspace with `skillshield-core` (a pure-ish library holding the scan → diff → notify pipeline as focused modules) and `skillshield-cli` (a thin `clap` binary that does arg parsing, interactive review, and orchestration). Periodic stateless execution: a Systemd timer/cron runs `skillshield scan`, which loads the baseline, walks the filesystem, diffs, notifies, and exits. Only interactive/explicit commands ever write the baseline.

**Tech Stack:** Rust (edition 2021), `clap` 4 (derive), `serde`/`serde_json`/`toml`, `sha2`, `walkdir`, `globset`, `directories`, `notify-rust`, `ureq`, `thiserror`, `tempfile`.

## Global Constraints

- Rust edition **2021**, MSRV **1.74+**.
- Dependency floors: `clap` 4, `serde` 1, `serde_json` 1, `toml` 0.8, `sha2` 0.10, `walkdir` 2, `globset` 0.4, `directories` 5, `notify-rust` 4, `ureq` 2 (with `json` feature), `thiserror` 1, `tempfile` 3.
- **Never follow symlinks** anywhere in the filesystem walk.
- **`scan` and `status` are strictly read-only** against the baseline — they must never write it.
- All state files (`baseline.json`, `config.toml`, `last-report.json`) written **atomically** (temp file + rename) with `0600` permissions, under XDG dirs (`$XDG_CONFIG_HOME`/`$XDG_DATA_HOME` with `~/.config`, `~/.local/share` fallbacks).
- **Fail loud:** unreadable locations, corrupt/tampered baseline, and operational errors are reported, never silently swallowed.
- Digests are strings of the form `"sha256:<hex>"`.
- Timestamps are `u64` Unix epoch seconds (from `SystemTime`), no date library.
- **Nothing user-specific in defaults:** `project_roots` defaults to empty.
- TDD throughout: failing test first, minimal implementation, passing test, commit. Run `cargo test` from the workspace root.

**Scope note for reviewer:** The spec lists email supporting "local sendmail or SMTP". This plan implements the **sendmail shell-out** path only (Task 10); SMTP-via-`lettre` is deliberately deferred as a follow-up to avoid a heavy dependency. Flag if SMTP is required for v1.

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/skillshield-core/Cargo.toml`
- Create: `crates/skillshield-core/src/lib.rs`
- Create: `crates/skillshield-cli/Cargo.toml`
- Create: `crates/skillshield-cli/src/main.rs`
- Create: `.gitignore`

**Interfaces:**
- Produces: workspace crates `skillshield-core` (lib) and `skillshield-cli` (bin `skillshield`).

- [ ] **Step 1: Create the `.gitignore`**

```
/target
```

- [ ] **Step 2: Create the workspace root `Cargo.toml`**

```toml
[workspace]
members = ["crates/skillshield-core", "crates/skillshield-cli"]
resolver = "2"

[workspace.package]
edition = "2021"
rust-version = "1.74"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
sha2 = "0.10"
walkdir = "2"
globset = "0.4"
directories = "5"
notify-rust = "4"
ureq = { version = "2", features = ["json"] }
thiserror = "1"
tempfile = "3"
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 3: Create `crates/skillshield-core/Cargo.toml`**

```toml
[package]
name = "skillshield-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
sha2.workspace = true
walkdir.workspace = true
globset.workspace = true
directories.workspace = true
notify-rust.workspace = true
ureq.workspace = true
thiserror.workspace = true
tempfile.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 4: Create `crates/skillshield-core/src/lib.rs`**

```rust
//! SkillShield core: scan → diff → notify pipeline.

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 5: Create `crates/skillshield-cli/Cargo.toml`**

```toml
[package]
name = "skillshield-cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "skillshield"
path = "src/main.rs"

[dependencies]
skillshield-core = { path = "../skillshield-core" }
clap.workspace = true
```

- [ ] **Step 6: Create `crates/skillshield-cli/src/main.rs`**

```rust
fn main() {
    println!("skillshield");
}
```

- [ ] **Step 7: Build and test**

Run: `cargo test`
Expected: PASS (1 test `workspace_builds`), both crates compile.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "chore: scaffold cargo workspace"
```

---

### Task 2: Error type and XDG/path resolution

**Files:**
- Create: `crates/skillshield-core/src/error.rs`
- Create: `crates/skillshield-core/src/paths.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum Error` (via `thiserror`) with variants `Io(std::io::Error)`, `Serde(String)`, `Corrupt(String)`, `NoHome`, `Other(String)`; `pub type Result<T> = std::result::Result<T, Error>`.
  - `paths::expand_tilde(&str) -> PathBuf`
  - `paths::config_path() -> Result<PathBuf>` → `<config>/skillshield/config.toml`
  - `paths::baseline_path() -> Result<PathBuf>` → `<data>/skillshield/baseline.json`
  - `paths::report_path() -> Result<PathBuf>` → `<data>/skillshield/last-report.json`
  - `paths::home_dir() -> Result<PathBuf>`

- [ ] **Step 1: Write failing tests**

Create `crates/skillshield-core/src/paths.rs` with a test module:

```rust
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expands_to_home() {
        let home = home_dir().unwrap();
        assert_eq!(expand_tilde("~/foo"), home.join("foo"));
    }

    #[test]
    fn tilde_bare_is_home() {
        assert_eq!(expand_tilde("~"), home_dir().unwrap());
    }

    #[test]
    fn non_tilde_is_verbatim() {
        assert_eq!(expand_tilde("/etc/foo"), PathBuf::from("/etc/foo"));
    }

    #[test]
    fn config_path_ends_correctly() {
        let p = config_path().unwrap();
        assert!(p.ends_with("skillshield/config.toml"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skillshield-core paths`
Expected: FAIL — `expand_tilde`/`home_dir`/`config_path` not found.

- [ ] **Step 3: Create `crates/skillshield-core/src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("corrupt or tampered data: {0}")]
    Corrupt(String),
    #[error("could not determine home directory")]
    NoHome,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 4: Implement `paths.rs` (prepend above the test module)**

```rust
use crate::error::{Error, Result};
use directories::BaseDirs;
use std::path::{Path, PathBuf};

pub fn home_dir() -> Result<PathBuf> {
    BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or(Error::NoHome)
}

pub fn expand_tilde(s: &str) -> PathBuf {
    if s == "~" {
        return home_dir().unwrap_or_else(|_| PathBuf::from("~"));
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

fn app_subdir(base: &Path) -> PathBuf {
    base.join("skillshield")
}

pub fn config_path() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(app_subdir(b.config_dir()).join("config.toml"))
}

pub fn baseline_path() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(app_subdir(b.data_dir()).join("baseline.json"))
}

pub fn report_path() -> Result<PathBuf> {
    let b = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(app_subdir(b.data_dir()).join("last-report.json"))
}
```

Remove the now-unused `use std::path::PathBuf;` line at the very top of the file (it's re-imported inside the impl block).

- [ ] **Step 5: Wire modules in `lib.rs`**

```rust
//! SkillShield core: scan → diff → notify pipeline.

pub mod error;
pub mod paths;

pub use error::{Error, Result};
```

(Delete the old `smoke` test module.)

- [ ] **Step 6: Run tests**

Run: `cargo test -p skillshield-core paths`
Expected: PASS (4 tests).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(core): error type and XDG path resolution"
```

---

### Task 3: Entry types and streaming hasher

**Files:**
- Create: `crates/skillshield-core/src/entry.rs`
- Create: `crates/skillshield-core/src/hashing.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Consumes: `Result` from Task 2.
- Produces:
  - `entry::EntryKind` (`File | Symlink`), `serde`-serializable.
  - `entry::Entry { path: PathBuf, kind: EntryKind, digest: Option<String>, symlink_target: Option<String>, size: u64, mtime: u64, unhashed: bool, source_rule: String }`, `serde`-serializable, `Clone`, `PartialEq`.
  - `hashing::hash_file(path: &Path, max_bytes: u64) -> Result<HashOutcome>` — hashes the file at `path` (uses `symlink_metadata` for the size guard, so it is only used on regular files by callers).
  - `hashing::hash_symlink_target(link_path: &Path, max_bytes: u64) -> Result<HashOutcome>` — follows the symlink (one dereference, possibly through a chain) and hashes the resolved **regular-file** contents. Returns `digest = None` for a dangling target, a directory, or a special file; respects `max_bytes` (oversized → `unhashed = true`).
  - `hashing::HashOutcome { digest: Option<String>, size: u64, unhashed: bool }` — `digest` is `Some("sha256:...")` unless the file exceeds `max_bytes`, in which case `digest = None, unhashed = true`.

- [ ] **Step 1: Write failing tests in `hashing.rs`**

```rust
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
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core hashing`
Expected: FAIL — `hash_file` not found.

- [ ] **Step 3: Create `entry.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: EntryKind,
    pub digest: Option<String>,
    pub symlink_target: Option<String>,
    pub size: u64,
    pub mtime: u64,
    pub unhashed: bool,
    pub source_rule: String,
}
```

- [ ] **Step 4: Implement `hashing.rs` (prepend above the test module)**

```rust
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
        return Ok(HashOutcome { digest: None, size, unhashed: true });
    }
    Ok(HashOutcome { digest: Some(stream_digest(path)?), size, unhashed: false })
}

/// Hash the regular-file contents a symlink resolves to (one dereference via
/// `std::fs::metadata`, which follows a chain to the final target). Returns
/// `digest = None` for a dangling target, a directory, or a special file.
pub fn hash_symlink_target(link_path: &Path, max_bytes: u64) -> Result<HashOutcome> {
    let meta = match std::fs::metadata(link_path) {
        Ok(m) => m,
        // Dangling or unreadable target: record target string only (no digest).
        Err(_) => return Ok(HashOutcome { digest: None, size: 0, unhashed: false }),
    };
    if !meta.file_type().is_file() {
        // Symlink to a directory or special file: do not traverse/hash.
        return Ok(HashOutcome { digest: None, size: meta.len(), unhashed: false });
    }
    let size = meta.len();
    if size > max_bytes {
        return Ok(HashOutcome { digest: None, size, unhashed: true });
    }
    Ok(HashOutcome { digest: Some(stream_digest(link_path)?), size, unhashed: false })
}
```

- [ ] **Step 5: Wire modules in `lib.rs`**

Add:

```rust
pub mod entry;
pub mod hashing;

pub use entry::{Entry, EntryKind};
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p skillshield-core`
Expected: PASS (hashing + paths tests).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(core): entry types and streaming sha256 hasher"
```

---

### Task 4: Catalog

**Files:**
- Create: `crates/skillshield-core/src/catalog.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `catalog::Scope` (`Global | Project`).
  - `catalog::MatchSpec` (`ExactPath(String) | Glob(String) | DirFileSet(String)`).
  - `catalog::Rule { id: String, description: String, spec: MatchSpec, scope: Scope }`.
  - `catalog::Catalog { rules: Vec<Rule> }` with `Catalog::builtin() -> Catalog`.
  - `catalog::default_rules() -> Vec<Rule>` — the curated defaults from the spec (17 global + 9 project rules).
  - `Catalog::apply(disable: &[String], extra_files: &[String]) -> Catalog` — removes rules whose `id` is in `disable`, and appends one `Project`-scope `Glob` rule per `extra_files` entry (id `extra.<n>`).

- [ ] **Step 1: Write failing tests in `catalog.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_known_rules() {
        let c = Catalog::builtin();
        assert!(c.rules.iter().any(|r| r.id == "claude.skills"));
        assert!(c.rules.iter().any(|r| r.id == "proj.agents.md"));
    }

    #[test]
    fn ids_are_unique() {
        let c = Catalog::builtin();
        let mut ids: Vec<_> = c.rules.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate rule ids");
    }

    #[test]
    fn apply_disables_and_extends() {
        let c = Catalog::builtin().apply(
            &["claude.skills".to_string()],
            &["**/MYAGENT.md".to_string()],
        );
        assert!(!c.rules.iter().any(|r| r.id == "claude.skills"));
        assert!(c.rules.iter().any(|r| r.id == "extra.0"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core catalog`
Expected: FAIL — `Catalog` not found.

- [ ] **Step 3: Implement `catalog.rs` (prepend above the test module)**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Global,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchSpec {
    ExactPath(String),
    Glob(String),
    DirFileSet(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub description: String,
    pub spec: MatchSpec,
    pub scope: Scope,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub rules: Vec<Rule>,
}

impl Catalog {
    pub fn builtin() -> Self {
        Catalog { rules: default_rules() }
    }

    pub fn apply(mut self, disable: &[String], extra_files: &[String]) -> Self {
        self.rules.retain(|r| !disable.iter().any(|d| d == &r.id));
        for (i, glob) in extra_files.iter().enumerate() {
            self.rules.push(Rule {
                id: format!("extra.{i}"),
                description: format!("user extra: {glob}"),
                spec: MatchSpec::Glob(glob.clone()),
                scope: Scope::Project,
            });
        }
        self
    }
}

fn global(id: &str, desc: &str, spec: MatchSpec) -> Rule {
    Rule { id: id.into(), description: desc.into(), spec, scope: Scope::Global }
}

fn project(id: &str, desc: &str, spec: MatchSpec) -> Rule {
    Rule { id: id.into(), description: desc.into(), spec, scope: Scope::Project }
}

pub fn default_rules() -> Vec<Rule> {
    use MatchSpec::*;
    vec![
        // ---- Global locations ----
        global("claude.home", "Claude home top-level files", DirFileSet("~/.claude/".into())),
        global("claude.skills", "Claude skills", DirFileSet("~/.claude/skills/".into())),
        global("claude.plugins", "Claude plugins & marketplaces", DirFileSet("~/.claude/plugins/".into())),
        global("claude.commands", "Claude commands", DirFileSet("~/.claude/commands/".into())),
        global("claude.agents", "Claude agents", DirFileSet("~/.claude/agents/".into())),
        global("claude.md.home", "Global CLAUDE.md", ExactPath("~/.claude/CLAUDE.md".into())),
        global("claude.settings", "Claude settings", Glob("~/.claude/settings*.json".into())),
        global("claude.mcp", "Claude MCP/project registry", ExactPath("~/.claude.json".into())),
        global("claude.config.xdg", "Claude XDG config", DirFileSet("~/.config/claude/".into())),
        global("codex.home", "Codex home", DirFileSet("~/.codex/".into())),
        global("codex.config.xdg", "Codex XDG config", DirFileSet("~/.config/codex/".into())),
        global("gemini.home", "Gemini home", DirFileSet("~/.gemini/".into())),
        global("gemini.md.home", "Global GEMINI.md", ExactPath("~/.gemini/GEMINI.md".into())),
        global("gemini.config.xdg", "Gemini XDG config", DirFileSet("~/.config/gemini/".into())),
        global("cursor.home", "Cursor home (rules, MCP)", DirFileSet("~/.cursor/".into())),
        global("copilot.config.xdg", "GitHub Copilot config", DirFileSet("~/.config/github-copilot/".into())),
        global("mcp.config.xdg", "MCP XDG config", DirFileSet("~/.config/mcp/".into())),
        // ---- Project artifact patterns ----
        project("proj.claude.md", "Project CLAUDE.md", Glob("**/CLAUDE.md".into())),
        project("proj.claude.local", "Project CLAUDE.local.md", Glob("**/CLAUDE.local.md".into())),
        project("proj.agents.md", "Project AGENTS.md", Glob("**/AGENTS.md".into())),
        project("proj.gemini.md", "Project GEMINI.md", Glob("**/GEMINI.md".into())),
        project("proj.claude.dir", "Project .claude directory", DirFileSet("**/.claude/".into())),
        project("proj.cursor.dir", "Project .cursor directory", DirFileSet("**/.cursor/".into())),
        project("proj.cursorrules", "Project .cursorrules", Glob("**/.cursorrules".into())),
        project("proj.mcp.json", "Project .mcp.json", Glob("**/.mcp.json".into())),
        project("proj.github.copilot", "Copilot instructions", Glob("**/.github/copilot-instructions.md".into())),
    ]
}
```

- [ ] **Step 4: Wire in `lib.rs`**

```rust
pub mod catalog;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p skillshield-core catalog`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): built-in catalog of agent artifacts"
```

---

### Task 5: Config loading

**Files:**
- Create: `crates/skillshield-core/src/config.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Consumes: `paths` (Task 2).
- Produces:
  - `config::Config { scan: ScanConfig, catalog: CatalogConfig, notify: NotifyConfig }`, all `serde` with `#[serde(default)]`.
  - `config::ScanConfig { follow_symlinks: bool, max_hash_bytes: u64, project_roots: Vec<String>, project_max_depth: usize, ignore: Vec<String> }`.
  - `config::CatalogConfig { extra_files: Vec<String>, disable: Vec<String> }`.
  - `config::NotifyConfig { channels: Vec<String>, email: Option<EmailConfig>, webhook: Option<WebhookConfig> }`.
  - `config::EmailConfig { to: String, from: String, sendmail_path: String }`.
  - `config::WebhookConfig { url: String, headers: Vec<(String, String)> }`.
  - `Config::load_from(path: &Path) -> Result<Config>` (returns defaults if the file is absent).
  - `Config::load() -> Result<Config>` (uses `paths::config_path()`).
  - `Config::default()` gives the spec defaults (empty `project_roots`, `channels = ["report","stdout"]`, etc.).

- [ ] **Step 1: Write failing tests in `config.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert!(!c.scan.follow_symlinks);
        assert_eq!(c.scan.max_hash_bytes, 5_000_000);
        assert!(c.scan.project_roots.is_empty());
        assert_eq!(c.scan.project_max_depth, 6);
        assert_eq!(c.notify.channels, vec!["report", "stdout"]);
    }

    #[test]
    fn absent_file_yields_defaults() {
        let c = Config::load_from(std::path::Path::new("/nonexistent/xyz.toml")).unwrap();
        assert_eq!(c.notify.channels, vec!["report", "stdout"]);
    }

    #[test]
    fn partial_toml_overrides_only_named_fields() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "[scan]\nproject_roots = [\"~/work\"]\n").unwrap();
        let c = Config::load_from(f.path()).unwrap();
        assert_eq!(c.scan.project_roots, vec!["~/work"]);
        // untouched field keeps its default
        assert_eq!(c.scan.max_hash_bytes, 5_000_000);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core config`
Expected: FAIL — `Config` not found.

- [ ] **Step 3: Implement `config.rs` (prepend above the test module)**

```rust
use crate::error::{Error, Result};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub scan: ScanConfig,
    pub catalog: CatalogConfig,
    pub notify: NotifyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub follow_symlinks: bool,
    pub max_hash_bytes: u64,
    pub project_roots: Vec<String>,
    pub project_max_depth: usize,
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogConfig {
    pub extra_files: Vec<String>,
    pub disable: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    pub channels: Vec<String>,
    pub email: Option<EmailConfig>,
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub to: String,
    pub from: String,
    #[serde(default = "default_sendmail")]
    pub sendmail_path: String,
}

fn default_sendmail() -> String {
    "/usr/sbin/sendmail".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scan: ScanConfig::default(),
            catalog: CatalogConfig::default(),
            notify: NotifyConfig::default(),
        }
    }
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig {
            follow_symlinks: false,
            max_hash_bytes: 5_000_000,
            project_roots: Vec::new(),
            project_max_depth: 6,
            ignore: vec![
                "**/node_modules/**".into(),
                "**/.git/**".into(),
                "**/target/**".into(),
                "**/vendor/**".into(),
            ],
        }
    }
}

impl Default for NotifyConfig {
    fn default() -> Self {
        NotifyConfig {
            channels: vec!["report".into(), "stdout".into()],
            email: None,
            webhook: None,
        }
    }
}

impl Config {
    pub fn load_from(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| Error::Serde(e.to_string()))
    }

    pub fn load() -> Result<Config> {
        Self::load_from(&paths::config_path()?)
    }
}
```

- [ ] **Step 4: Wire in `lib.rs`**

```rust
pub mod config;

pub use config::Config;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p skillshield-core config`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): XDG-aware TOML config with defaults"
```

---

### Task 6: Discovery (filesystem walk)

**Files:**
- Create: `crates/skillshield-core/src/discovery.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Consumes: `Catalog`, `MatchSpec`, `Scope` (Task 4), `ScanConfig` (Task 5), `Entry`/`EntryKind` (Task 3), `hashing::{hash_file, hash_symlink_target}` (Task 3), `paths::expand_tilde` (Task 2).
- Produces:
  - `discovery::ScanError { path: PathBuf, message: String }` (`serde`).
  - `discovery::Scan { entries: Vec<Entry>, errors: Vec<ScanError> }` — `entries` sorted by path.
  - `discovery::discover(catalog: &Catalog, cfg: &ScanConfig) -> Scan`.
  - Never traverses *into* symlinked directories. A symlink produces an `Entry` with `kind = Symlink`, `symlink_target = Some(<literal target>)`, and a `digest` that is the one-hop hash of the resolved regular-file contents (via `hash_symlink_target`), or `None` for a directory/special/dangling target.
  - `Global` rules resolve their path via `expand_tilde` and are scanned directly. `Project` rules are matched (via `globset`) against files found by crawling each `cfg.project_roots` entry, honoring `cfg.project_max_depth` and `cfg.ignore`.

- [ ] **Step 1: Write failing integration-style tests in `discovery.rs`**

```rust
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
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core discovery`
Expected: FAIL — `discover` not found.

- [ ] **Step 3: Implement `discovery.rs` (prepend above the test module)**

```rust
use crate::catalog::{Catalog, MatchSpec, Rule, Scope};
use crate::config::ScanConfig;
use crate::entry::{Entry, EntryKind};
use crate::hashing::{hash_file, hash_symlink_target, HashOutcome};
use crate::paths::expand_tilde;
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
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
            let out = hash_symlink_target(path, self.max_hash_bytes)
                .unwrap_or(HashOutcome { digest: None, size: meta.len(), unhashed: false });
            self.entries.insert(path.into(), Entry {
                path: path.into(),
                kind: EntryKind::Symlink,
                digest: out.digest,
                symlink_target: target,
                size: meta.len(),
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
                            if let Ok(e) = e {
                                if (e.file_type().is_file() || e.file_type().is_symlink())
                                    && set.is_match(e.path())
                                {
                                    col.add_file(e.path(), &rule.id);
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
        MatchSpec::Glob(g) | MatchSpec::DirFileSet(g) => {
            build_globset(std::slice::from_ref(g))
                .map(|s| s.is_match(rel))
                .unwrap_or(false)
        }
        MatchSpec::ExactPath(_) => false,
    }
}

fn build_globset(patterns: &[String]) -> Result<globset::GlobSet, globset::Error> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p)?);
    }
    b.build()
}
```

Note on `DirFileSet` project rules (e.g. `**/.claude/`): the glob `**/.claude/` matches the directory path itself; files inside are matched because the crawl visits every file and `globset` with a trailing-slash pattern is treated as a prefix. To make in-directory files match reliably, the implementer must expand `DirFileSet("**/x/")` to also test the pattern `**/x/**`. Update `project_rule_matches`:

```rust
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
```

- [ ] **Step 4: Wire in `lib.rs`**

```rust
pub mod discovery;

pub use discovery::{discover, Scan, ScanError};
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p skillshield-core discovery`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): filesystem discovery (globals + project crawl)"
```

---

### Task 7: Baseline (load/save/integrity)

**Files:**
- Create: `crates/skillshield-core/src/baseline.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Consumes: `Entry` (Task 3), `Error`/`Result` (Task 2).
- Produces:
  - `baseline::Baseline { version: u32, entries: Vec<Entry> }` (entries kept sorted by path).
  - `Baseline::new(entries: Vec<Entry>) -> Baseline` (version = `CURRENT_VERSION` = 1).
  - `Baseline::load(path: &Path) -> Result<Baseline>` — verifies the on-disk integrity digest; returns `Error::Corrupt` on mismatch or bad version.
  - `Baseline::save(&self, path: &Path) -> Result<()>` — atomic write (temp + rename) with `0600`, embedding a recomputed integrity digest.
  - `Baseline::contains_path(&self, p: &Path) -> bool`.
  - `Baseline::upsert(&mut self, e: Entry)` (replace by path or insert, keep sorted).
  - `Baseline::remove_under(&mut self, prefix: &Path) -> usize` (remove entries at/under a prefix; returns count removed).
  - `Baseline::integrity_digest(&self) -> String` — `sha256:` over canonical JSON of `entries`.

- [ ] **Step 1: Write failing tests in `baseline.rs`**

```rust
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
        Baseline::new(vec![e("/a", "sha256:1")]).save(&path).unwrap();

        // Tamper: flip a digest but leave the stored integrity digest alone.
        let text = std::fs::read_to_string(&path).unwrap();
        let tampered = text.replace("sha256:1", "sha256:evil");
        std::fs::write(&path, tampered).unwrap();

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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core baseline`
Expected: FAIL — `Baseline` not found.

- [ ] **Step 3: Implement `baseline.rs` (prepend above the test module)**

```rust
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
        Baseline { version: CURRENT_VERSION, entries }
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
        let text = std::fs::read_to_string(path)?;
        let disk: OnDisk =
            serde_json::from_str(&text).map_err(|e| Error::Corrupt(e.to_string()))?;
        if disk.version != CURRENT_VERSION {
            return Err(Error::Corrupt(format!(
                "unsupported baseline version {}",
                disk.version
            )));
        }
        let b = Baseline { version: disk.version, entries: disk.entries };
        if b.integrity_digest() != disk.integrity {
            return Err(Error::Corrupt("baseline integrity digest mismatch".into()));
        }
        Ok(b)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let disk = OnDisk {
            version: self.version,
            integrity: self.integrity_digest(),
            entries: self.entries.clone(),
        };
        let json = serde_json::to_vec_pretty(&disk).map_err(|e| Error::Serde(e.to_string()))?;

        let dir = path.parent().unwrap_or_else(|| Path::new("."));
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
```

- [ ] **Step 4: Wire in `lib.rs`**

```rust
pub mod baseline;

pub use baseline::Baseline;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p skillshield-core baseline`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): baseline persistence with integrity digest"
```

---

### Task 8: Diff and ScanReport

**Files:**
- Create: `crates/skillshield-core/src/diff.rs`
- Create: `crates/skillshield-core/src/report.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Consumes: `Baseline` (Task 7), `Scan`/`ScanError` (Task 6), `Entry`/`EntryKind` (Task 3).
- Produces:
  - `diff::ChangeKind` (`Added | Modified | Removed`), `serde`.
  - `diff::Finding { path: PathBuf, change: ChangeKind, kind: EntryKind, rule_id: String, old_digest: Option<String>, new_digest: Option<String>, detail: String }`, `serde`.
  - `diff::ScanDiff { findings: Vec<Finding> }` with `ScanDiff::is_empty(&self) -> bool`.
  - `diff::diff(baseline: &Baseline, scan: &Scan) -> ScanDiff`.
  - A file is `Modified` if its `digest` differs, OR its `symlink_target` differs, OR its `unhashed` flag differs.
  - `report::ScanReport { findings: Vec<Finding>, scan_errors: Vec<ScanError>, added: usize, modified: usize, removed: usize, generated_at: u64 }`, `serde`.
  - `report::ScanReport::from_diff(diff: &ScanDiff, errors: &[ScanError], now: u64) -> ScanReport`.
  - `report::now_secs() -> u64` (SystemTime → Unix seconds).

- [ ] **Step 1: Write failing tests in `diff.rs`**

```rust
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core diff`
Expected: FAIL — `diff` not found.

- [ ] **Step 3: Implement `diff.rs` (prepend above the test module)**

```rust
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
```

- [ ] **Step 4: Implement `report.rs`**

```rust
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
```

- [ ] **Step 5: Wire in `lib.rs`**

```rust
pub mod diff;
pub mod report;

pub use diff::{diff, ChangeKind, Finding, ScanDiff};
pub use report::ScanReport;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p skillshield-core`
Expected: PASS (all core tests).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(core): diff engine and scan report"
```

---

### Task 9: Notifier trait, registry, `report` + `stdout` channels

**Files:**
- Create: `crates/skillshield-core/src/notify/mod.rs`
- Create: `crates/skillshield-core/src/notify/report_file.rs`
- Create: `crates/skillshield-core/src/notify/stdout.rs`
- Modify: `crates/skillshield-core/src/lib.rs`

**Interfaces:**
- Consumes: `ScanReport` (Task 8), `NotifyConfig` (Task 5), `paths::report_path` (Task 2).
- Produces:
  - `notify::NotifyError { channel: String, message: String }` (impl `std::fmt::Display`).
  - `notify::Notifier` trait: `fn id(&self) -> &str; fn notify(&self, report: &ScanReport) -> std::result::Result<(), NotifyError>;`
  - `notify::render_text(report: &ScanReport) -> String` — shared human-readable rendering.
  - `notify::build_notifiers(cfg: &NotifyConfig) -> Vec<Box<dyn Notifier>>` — maps each id in `cfg.channels` to a channel; unknown ids are skipped with an eprintln warning.
  - `notify::dispatch(notifiers: &[Box<dyn Notifier>], report: &ScanReport) -> Vec<NotifyError>` — runs each independently, collecting (not propagating) errors.
  - `notify::report_file::ReportFileNotifier` (writes `last-report.json` atomically, 0600) and `notify::stdout::StdoutNotifier`.

- [ ] **Step 1: Write failing tests in `notify/mod.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ChangeKind, Finding, ScanDiff};
    use crate::entry::EntryKind;
    use crate::report::ScanReport;

    fn sample_report() -> ScanReport {
        let f = Finding {
            path: "/x/CLAUDE.md".into(),
            change: ChangeKind::Added,
            kind: EntryKind::File,
            rule_id: "proj.claude.md".into(),
            old_digest: None,
            new_digest: Some("sha256:ab".into()),
            detail: "new file".into(),
        };
        ScanReport::from_diff(&ScanDiff { findings: vec![f] }, &[], 1000)
    }

    struct Failing;
    impl Notifier for Failing {
        fn id(&self) -> &str { "failing" }
        fn notify(&self, _r: &ScanReport) -> std::result::Result<(), NotifyError> {
            Err(NotifyError { channel: "failing".into(), message: "boom".into() })
        }
    }
    struct Ok;
    impl Notifier for Ok {
        fn id(&self) -> &str { "ok" }
        fn notify(&self, _r: &ScanReport) -> std::result::Result<(), NotifyError> { std::result::Result::Ok(()) }
    }

    #[test]
    fn dispatch_isolates_failures() {
        let notifiers: Vec<Box<dyn Notifier>> = vec![Box::new(Failing), Box::new(Ok)];
        let errs = dispatch(&notifiers, &sample_report());
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].channel, "failing");
    }

    #[test]
    fn render_text_mentions_counts_and_path() {
        let text = render_text(&sample_report());
        assert!(text.contains("Added: 1"));
        assert!(text.contains("CLAUDE.md"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core notify`
Expected: FAIL — `Notifier` not found.

- [ ] **Step 3: Implement `notify/mod.rs` (prepend above the test module)**

```rust
pub mod report_file;
pub mod stdout;

use crate::config::NotifyConfig;
use crate::report::ScanReport;

#[derive(Debug, Clone)]
pub struct NotifyError {
    pub channel: String,
    pub message: String,
}

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.channel, self.message)
    }
}

pub trait Notifier {
    fn id(&self) -> &str;
    fn notify(&self, report: &ScanReport) -> std::result::Result<(), NotifyError>;
}

pub fn render_text(report: &ScanReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "SkillShield: {} change(s) — Added: {}, Modified: {}, Removed: {}\n",
        report.findings.len(),
        report.added,
        report.modified,
        report.removed
    ));
    for f in &report.findings {
        s.push_str(&format!(
            "  {:?}  {}  [{}]  {}\n",
            f.change,
            f.path.display(),
            f.rule_id,
            f.detail
        ));
    }
    if !report.scan_errors.is_empty() {
        s.push_str(&format!("  {} scan error(s):\n", report.scan_errors.len()));
        for e in &report.scan_errors {
            s.push_str(&format!("    ! {} — {}\n", e.path.display(), e.message));
        }
    }
    s
}

pub fn build_notifiers(cfg: &NotifyConfig) -> Vec<Box<dyn Notifier>> {
    let mut out: Vec<Box<dyn Notifier>> = Vec::new();
    for id in &cfg.channels {
        match id.as_str() {
            "report" => out.push(Box::new(report_file::ReportFileNotifier::default())),
            "stdout" => out.push(Box::new(stdout::StdoutNotifier)),
            "desktop" => out.push(Box::new(crate::notify::desktop::DesktopNotifier)),
            "webhook" => {
                if let Some(w) = &cfg.webhook {
                    out.push(Box::new(crate::notify::webhook::WebhookNotifier::new(w.clone())));
                } else {
                    eprintln!("skillshield: 'webhook' channel enabled but [notify.webhook] is missing");
                }
            }
            "email" => {
                if let Some(e) = &cfg.email {
                    out.push(Box::new(crate::notify::email::EmailNotifier::new(e.clone())));
                } else {
                    eprintln!("skillshield: 'email' channel enabled but [notify.email] is missing");
                }
            }
            other => eprintln!("skillshield: unknown notify channel '{other}', skipping"),
        }
    }
    out
}

pub fn dispatch(notifiers: &[Box<dyn Notifier>], report: &ScanReport) -> Vec<NotifyError> {
    let mut errors = Vec::new();
    for n in notifiers {
        if let Err(e) = n.notify(report) {
            errors.push(e);
        }
    }
    errors
}
```

Note: the `desktop`, `webhook`, and `email` arms reference modules created in Task 10. To keep this task compiling on its own, temporarily comment out those three match arms (leaving `report`, `stdout`, and the `other =>` catch-all) and uncomment them in Task 10 Step 4. The tests in this task only exercise `dispatch`/`render_text`, not those channels.

- [ ] **Step 4: Implement `notify/report_file.rs`**

```rust
use super::{render_text, NotifyError, Notifier};
use crate::paths;
use crate::report::ScanReport;
use std::io::Write;
use std::path::PathBuf;

pub struct ReportFileNotifier {
    pub path: Option<PathBuf>,
}

impl Default for ReportFileNotifier {
    fn default() -> Self {
        ReportFileNotifier { path: paths::report_path().ok() }
    }
}

impl Notifier for ReportFileNotifier {
    fn id(&self) -> &str {
        "report"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let err = |m: String| NotifyError { channel: "report".into(), message: m };
        let path = self.path.clone().ok_or_else(|| err("no report path".into()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| err(e.to_string()))?;
        }
        let json = serde_json::to_vec_pretty(report).map_err(|e| err(e.to_string()))?;
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| err(e.to_string()))?;
        tmp.write_all(&json).map_err(|e| err(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))
                .map_err(|e| err(e.to_string()))?;
        }
        tmp.persist(&path).map_err(|e| err(e.to_string()))?;

        // Also append a human-readable line-log next to the JSON.
        let log = path.with_file_name("skillshield.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            let _ = writeln!(f, "--- {} ---\n{}", report.generated_at, render_text(report));
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Implement `notify/stdout.rs`**

```rust
use super::{render_text, NotifyError, Notifier};
use crate::report::ScanReport;

pub struct StdoutNotifier;

impl Notifier for StdoutNotifier {
    fn id(&self) -> &str {
        "stdout"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        if report.has_changes() || !report.scan_errors.is_empty() {
            print!("{}", render_text(report));
        } else {
            println!("SkillShield: no changes.");
        }
        Ok(())
    }
}
```

- [ ] **Step 6: Wire in `lib.rs`**

```rust
pub mod notify;
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p skillshield-core notify`
Expected: PASS (2 tests).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(core): notifier trait, registry, report+stdout channels"
```

---

### Task 10: `desktop`, `webhook`, `email` channels

**Files:**
- Create: `crates/skillshield-core/src/notify/desktop.rs`
- Create: `crates/skillshield-core/src/notify/webhook.rs`
- Create: `crates/skillshield-core/src/notify/email.rs`
- Modify: `crates/skillshield-core/src/notify/mod.rs`

**Interfaces:**
- Consumes: `WebhookConfig`, `EmailConfig` (Task 5), `ScanReport` (Task 8), `render_text` (Task 9).
- Produces:
  - `notify::desktop::DesktopNotifier` (uses `notify-rust`; on non-graphical failure returns a `NotifyError` — dispatch isolates it).
  - `notify::webhook::WebhookNotifier::new(cfg: WebhookConfig)` — POSTs the JSON `ScanReport` to `cfg.url` with `cfg.headers`.
  - `notify::email::EmailNotifier::new(cfg: EmailConfig)` — pipes a plaintext message to `cfg.sendmail_path -t`.
  - `notify::desktop::graphical_session_available() -> bool` — true if `$DISPLAY` or `$WAYLAND_DISPLAY` is set (used by `init` in Task 12).

- [ ] **Step 1: Write failing tests**

In `notify/desktop.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_graphical_env() {
        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
        assert!(graphical_session_available());
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        assert!(!graphical_session_available());
    }
}
```

In `notify/email.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmailConfig;
    use crate::diff::ScanDiff;
    use crate::report::ScanReport;

    #[test]
    fn builds_rfc822_message_with_headers() {
        let cfg = EmailConfig { to: "me@example.com".into(), from: "ss@host".into(), sendmail_path: "/bin/true".into() };
        let n = EmailNotifier::new(cfg);
        let report = ScanReport::from_diff(&ScanDiff { findings: vec![] }, &[], 42);
        let msg = n.build_message(&report);
        assert!(msg.starts_with("To: me@example.com\r\n"));
        assert!(msg.contains("From: ss@host\r\n"));
        assert!(msg.contains("Subject: SkillShield"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-core notify::desktop notify::email`
Expected: FAIL — modules not found.

- [ ] **Step 3a: Implement `notify/desktop.rs`**

```rust
use super::{render_text, NotifyError, Notifier};
use crate::report::ScanReport;

pub struct DesktopNotifier;

pub fn graphical_session_available() -> bool {
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

impl Notifier for DesktopNotifier {
    fn id(&self) -> &str {
        "desktop"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let err = |m: String| NotifyError { channel: "desktop".into(), message: m };
        if !report.has_changes() {
            return Ok(());
        }
        let summary = format!(
            "SkillShield: {} change(s)",
            report.findings.len()
        );
        notify_rust::Notification::new()
            .summary(&summary)
            .body(&render_text(report))
            .show()
            .map_err(|e| err(e.to_string()))?;
        Ok(())
    }
}
```

- [ ] **Step 3b: Implement `notify/webhook.rs`**

```rust
use super::{NotifyError, Notifier};
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
    fn id(&self) -> &str {
        "webhook"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let err = |m: String| NotifyError { channel: "webhook".into(), message: m };
        let mut req = ureq::post(&self.cfg.url);
        for (k, v) in &self.cfg.headers {
            req = req.set(k, v);
        }
        req.send_json(serde_json::to_value(report).map_err(|e| err(e.to_string()))?)
            .map_err(|e| err(e.to_string()))?;
        Ok(())
    }
}
```

- [ ] **Step 3c: Implement `notify/email.rs`**

```rust
use super::{render_text, NotifyError, Notifier};
use crate::config::EmailConfig;
use crate::report::ScanReport;
use std::io::Write;
use std::process::{Command, Stdio};

pub struct EmailNotifier {
    cfg: EmailConfig,
}

impl EmailNotifier {
    pub fn new(cfg: EmailConfig) -> Self {
        EmailNotifier { cfg }
    }

    pub fn build_message(&self, report: &ScanReport) -> String {
        format!(
            "To: {}\r\nFrom: {}\r\nSubject: SkillShield: {} change(s)\r\n\r\n{}",
            self.cfg.to,
            self.cfg.from,
            report.findings.len(),
            render_text(report)
        )
    }
}

impl Notifier for EmailNotifier {
    fn id(&self) -> &str {
        "email"
    }

    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError> {
        let err = |m: String| NotifyError { channel: "email".into(), message: m };
        if !report.has_changes() {
            return Ok(());
        }
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
}
```

- [ ] **Step 4: Enable the channels in `notify/mod.rs`**

Add module declarations at the top of `notify/mod.rs`:

```rust
pub mod desktop;
pub mod email;
pub mod webhook;
```

Uncomment the `desktop`, `webhook`, and `email` match arms in `build_notifiers` (added in Task 9 Step 3).

- [ ] **Step 5: Run tests**

Run: `cargo test -p skillshield-core`
Expected: PASS (desktop + email tests, all prior tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): desktop, webhook, and email notify channels"
```

---

### Task 11: CLI scaffold, argument parsing, exit codes

**Files:**
- Create: `crates/skillshield-cli/src/cli.rs`
- Create: `crates/skillshield-cli/src/exit.rs`
- Create: `crates/skillshield-cli/src/commands/mod.rs`
- Modify: `crates/skillshield-cli/src/main.rs`

**Interfaces:**
- Consumes: nothing from core yet (dispatch stubs).
- Produces:
  - `cli::Cli` (clap `Parser`) with `command: Command`.
  - `cli::Command` enum: `Init { force: bool }`, `Scan`, `Status`, `Review`, `Trust { path: PathBuf }`, `Monitor { path: PathBuf }`, `Unmonitor { path: PathBuf }`.
  - `exit::Code` with associated consts `OK = 0`, `CHANGES = 10`, `ERROR = 1`, and `exit::finish(result: Result<i32, String>) -> !` that prints errors to stderr and calls `std::process::exit`.
  - `commands::mod` with one `pub fn run_*` per command returning `Result<i32, String>` (stubs returning `Ok(0)` for now, real bodies in later tasks).

- [ ] **Step 1: Write a failing test in `cli.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_scan() {
        let cli = Cli::try_parse_from(["skillshield", "scan"]).unwrap();
        assert!(matches!(cli.command, Command::Scan));
    }

    #[test]
    fn parses_trust_with_path() {
        let cli = Cli::try_parse_from(["skillshield", "trust", "/a/b"]).unwrap();
        match cli.command {
            Command::Trust { path } => assert_eq!(path, std::path::PathBuf::from("/a/b")),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn init_force_flag() {
        let cli = Cli::try_parse_from(["skillshield", "init", "--force"]).unwrap();
        assert!(matches!(cli.command, Command::Init { force: true }));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-cli cli`
Expected: FAIL — `Cli` not found.

- [ ] **Step 3: Implement `cli.rs` (prepend above the test module)**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "skillshield", version, about = "Tripwire for AI-agent config artifacts")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build the trusted baseline (first run).
    Init {
        /// Overwrite an existing baseline.
        #[arg(long)]
        force: bool,
    },
    /// Scan and report changes vs. the baseline (scheduled use). Read-only.
    Scan,
    /// Show current changes vs. the baseline. Read-only.
    Status,
    /// Interactively accept/reject pending changes.
    Review,
    /// Accept a specific path into the baseline.
    Trust {
        path: PathBuf,
    },
    /// Add a project root: crawl once, record in config, trust findings.
    Monitor {
        path: PathBuf,
    },
    /// Remove a project root from config and prune its baseline entries.
    Unmonitor {
        path: PathBuf,
    },
}
```

- [ ] **Step 4: Implement `exit.rs`**

```rust
pub struct Code;

impl Code {
    pub const OK: i32 = 0;
    pub const CHANGES: i32 = 10;
    pub const ERROR: i32 = 1;
}

pub fn finish(result: Result<i32, String>) -> ! {
    match result {
        Ok(code) => std::process::exit(code),
        Err(msg) => {
            eprintln!("skillshield: error: {msg}");
            std::process::exit(Code::ERROR);
        }
    }
}
```

- [ ] **Step 5: Implement `commands/mod.rs` (stubs)**

```rust
use crate::cli::Command;
use std::path::Path;

pub mod init;
pub mod scan;
pub mod status;
pub mod review;
pub mod trust;
pub mod monitor;

pub fn run(command: Command) -> Result<i32, String> {
    match command {
        Command::Init { force } => init::run(force),
        Command::Scan => scan::run(),
        Command::Status => status::run(),
        Command::Review => review::run(),
        Command::Trust { path } => trust::run(&path),
        Command::Monitor { path } => monitor::run(&path),
        Command::Unmonitor { path } => monitor::run_unmonitor(&path),
    }
}

// helper reused by several commands
pub fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn abs(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
```

Create stub files so the crate compiles. Each is:

`commands/init.rs`:
```rust
pub fn run(_force: bool) -> Result<i32, String> {
    Ok(0)
}
```
`commands/scan.rs`:
```rust
pub fn run() -> Result<i32, String> {
    Ok(0)
}
```
`commands/status.rs`:
```rust
pub fn run() -> Result<i32, String> {
    Ok(0)
}
```
`commands/review.rs`:
```rust
pub fn run() -> Result<i32, String> {
    Ok(0)
}
```
`commands/trust.rs`:
```rust
use std::path::Path;
pub fn run(_path: &Path) -> Result<i32, String> {
    Ok(0)
}
```
`commands/monitor.rs`:
```rust
use std::path::Path;
pub fn run(_path: &Path) -> Result<i32, String> {
    Ok(0)
}
pub fn run_unmonitor(_path: &Path) -> Result<i32, String> {
    Ok(0)
}
```

- [ ] **Step 6: Rewrite `main.rs`**

```rust
mod cli;
mod commands;
mod exit;

use clap::Parser;

fn main() {
    let parsed = cli::Cli::parse();
    exit::finish(commands::run(parsed.command));
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p skillshield-cli`
Expected: PASS (3 cli tests).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(cli): argument parsing, exit codes, command dispatch"
```

---

### Task 12: `init` command + review grouping logic

**Files:**
- Create: `crates/skillshield-cli/src/review_ui.rs`
- Modify: `crates/skillshield-cli/src/commands/init.rs`
- Modify: `crates/skillshield-cli/src/main.rs` (add `mod review_ui;`)
- Modify: `crates/skillshield-cli/Cargo.toml` (no new deps; uses `skillshield-core`)

**Interfaces:**
- Consumes: `Catalog`, `Config`, `discover`, `Baseline`, `Entry`, `paths`, `notify::desktop::graphical_session_available` (all core).
- Produces:
  - `review_ui::Group { key: String, entries_idx: Vec<usize> }`.
  - `review_ui::group_entries(entries: &[Entry]) -> Vec<Group>` — groups by `source_rule`, ordered by first appearance. Pure and unit-tested.
  - `init::run(force: bool) -> Result<i32, String>` — refuses if baseline exists and `!force`; otherwise discovers, prints grouped summary, prompts (trust-all / per-group / drill-in), writes baseline, and if `graphical_session_available()` ensures `desktop` is in config channels + sends a test notification.

- [ ] **Step 1: Write a failing test in `review_ui.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use skillshield_core::entry::{Entry, EntryKind};

    fn e(path: &str, rule: &str) -> Entry {
        Entry {
            path: path.into(), kind: EntryKind::File, digest: Some("sha256:1".into()),
            symlink_target: None, size: 1, mtime: 0, unhashed: false, source_rule: rule.into(),
        }
    }

    #[test]
    fn groups_by_source_rule() {
        let entries = vec![e("/a", "claude.skills"), e("/b", "claude.skills"), e("/c", "codex.home")];
        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "claude.skills");
        assert_eq!(groups[0].entries_idx, vec![0, 1]);
        assert_eq!(groups[1].key, "codex.home");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-cli review_ui`
Expected: FAIL — `group_entries` not found.

- [ ] **Step 3: Implement `review_ui.rs` (prepend above the test module)**

```rust
use skillshield_core::entry::Entry;

pub struct Group {
    pub key: String,
    pub entries_idx: Vec<usize>,
}

pub fn group_entries(entries: &[Entry]) -> Vec<Group> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        let key = e.source_rule.clone();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(i);
    }
    order
        .into_iter()
        .map(|key| {
            let entries_idx = groups.remove(&key).unwrap();
            Group { key, entries_idx }
        })
        .collect()
}
```

- [ ] **Step 4: Implement `init::run` in `commands/init.rs`**

```rust
use crate::commands::to_err;
use crate::review_ui::group_entries;
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::Catalog;
use skillshield_core::config::Config;
use skillshield_core::discovery::discover;
use skillshield_core::notify::desktop::{graphical_session_available, DesktopNotifier};
use skillshield_core::notify::Notifier;
use skillshield_core::report::{now_secs, ScanReport};
use skillshield_core::diff::ScanDiff;
use skillshield_core::paths;
use std::io::{self, Write};

pub fn run(force: bool) -> Result<i32, String> {
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    if baseline_path.exists() && !force {
        return Err(format!(
            "baseline already exists at {}. Use `skillshield scan`/`review`, or `init --force` to rebuild.",
            baseline_path.display()
        ));
    }

    let cfg = Config::load().map_err(to_err)?;
    let catalog = Catalog::builtin().apply(&cfg.catalog.disable, &cfg.catalog.extra_files);
    let scan = discover(&catalog, &cfg.scan);

    if scan.entries.is_empty() {
        println!("No agent artifacts found. Nothing to baseline yet.");
    }
    let groups = group_entries(&scan.entries);
    println!("Found {} file(s) across {} group(s):", scan.entries.len(), groups.len());
    for g in &groups {
        println!("  [{}] {} file(s)", g.key, g.entries_idx.len());
    }
    for e in &scan.errors {
        eprintln!("  ! could not read {} — {}", e.path.display(), e.message);
    }

    print!("Trust all discovered files as the baseline? [y/N] ");
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(to_err)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        println!("Aborted. No baseline written.");
        return Ok(0);
    }

    let baseline = Baseline::new(scan.entries.clone());
    baseline.save(&baseline_path).map_err(to_err)?;
    println!("Baseline written to {}", baseline_path.display());

    maybe_setup_desktop(&cfg)?;
    print_scheduling_hint();
    Ok(0)
}

fn maybe_setup_desktop(cfg: &Config) -> Result<(), String> {
    if !graphical_session_available() {
        println!(
            "No graphical session detected; 'desktop' notifications left disabled. \
             Add \"desktop\" to notify.channels in config.toml to enable."
        );
        return Ok(());
    }
    // Persist "desktop" into the config channels if missing.
    let config_path = paths::config_path().map_err(to_err)?;
    let mut cfg = cfg.clone();
    if !cfg.notify.channels.iter().any(|c| c == "desktop") {
        cfg.notify.channels.push("desktop".into());
        write_config(&config_path, &cfg)?;
        println!("Enabled 'desktop' notifications in {}", config_path.display());
    }
    // Send a one-off test notification.
    let test = ScanReport::from_diff(&ScanDiff { findings: vec![] }, &[], now_secs());
    let _ = DesktopNotifier.notify(&test); // best-effort
    println!("Sent a test desktop notification (check your notifications).");
    Ok(())
}

fn write_config(path: &std::path::Path, cfg: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(to_err)?;
    }
    let text = toml::to_string_pretty(cfg).map_err(to_err)?;
    let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(to_err)?;
    tmp.write_all(text.as_bytes()).map_err(to_err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600)).map_err(to_err)?;
    }
    tmp.persist(path).map_err(|e| to_err(e.error))?;
    Ok(())
}

fn print_scheduling_hint() {
    println!(
        "\nTo run periodically:\n  systemctl --user enable --now skillshield.timer\n  \
         (or add a cron entry — see packaging/ in the repo)"
    );
}
```

Note: `init` sends the test notification with an empty report; `DesktopNotifier::notify` returns early on no-changes. For `init`'s test ping, call the underlying `notify_rust` directly instead. Replace the test-notification line with:

```rust
    let _ = notify_rust::Notification::new()
        .summary("SkillShield")
        .body("Desktop notifications are working.")
        .show();
```

and remove the now-unused `ScanReport`/`ScanDiff`/`now_secs` imports if the compiler warns. Add `notify-rust` to `skillshield-cli`'s dependencies:

In `crates/skillshield-cli/Cargo.toml` under `[dependencies]`:
```toml
notify-rust.workspace = true
toml.workspace = true
tempfile.workspace = true
serde_json.workspace = true
```

- [ ] **Step 5: Add `mod review_ui;` to `main.rs`**

```rust
mod cli;
mod commands;
mod exit;
mod review_ui;
```

- [ ] **Step 6: Run tests + manual smoke**

Run: `cargo test -p skillshield-cli`
Expected: PASS (review_ui + cli tests).

Manual (optional): `cargo run -p skillshield-cli -- init` in a throwaway `HOME` and confirm it lists groups and prompts.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(cli): init command with grouped review and desktop setup"
```

---

### Task 13: `scan` and `status` commands

**Files:**
- Modify: `crates/skillshield-cli/src/commands/scan.rs`
- Modify: `crates/skillshield-cli/src/commands/status.rs`

**Interfaces:**
- Consumes: `Config`, `Catalog`, `discover`, `Baseline`, `diff`, `ScanReport`, `notify::{build_notifiers, dispatch}` (core); `exit::Code`.
- Produces:
  - `scan::run() -> Result<i32, String>`: load baseline (error if missing → tells user to run `init`), discover, diff, build report, dispatch notifiers, return `Code::CHANGES` (10) if any findings else `Code::OK` (0). **Never writes the baseline.**
  - `status::run() -> Result<i32, String>`: same discovery+diff but prints `render_text` to stdout only, no notifiers, same exit-code convention. **Never writes the baseline.**
  - Shared helper `commands::load_baseline_or_hint() -> Result<Baseline, String>`.

- [ ] **Step 1: Add shared helper to `commands/mod.rs`**

```rust
pub fn load_baseline_or_hint() -> Result<skillshield_core::baseline::Baseline, String> {
    let path = skillshield_core::paths::baseline_path().map_err(to_err)?;
    if !path.exists() {
        return Err(format!(
            "no baseline at {}. Run `skillshield init` first.",
            path.display()
        ));
    }
    skillshield_core::baseline::Baseline::load(&path).map_err(to_err)
}

pub fn discover_now(
) -> Result<(skillshield_core::discovery::Scan, skillshield_core::config::Config), String> {
    let cfg = skillshield_core::config::Config::load().map_err(to_err)?;
    let catalog = skillshield_core::catalog::Catalog::builtin()
        .apply(&cfg.catalog.disable, &cfg.catalog.extra_files);
    Ok((skillshield_core::discovery::discover(&catalog, &cfg.scan), cfg))
}
```

- [ ] **Step 2: Implement `commands/scan.rs`**

```rust
use crate::commands::{discover_now, load_baseline_or_hint, to_err};
use crate::exit::Code;
use skillshield_core::diff::diff;
use skillshield_core::notify::{build_notifiers, dispatch};
use skillshield_core::report::{now_secs, ScanReport};

pub fn run() -> Result<i32, String> {
    let baseline = load_baseline_or_hint()?;
    let (scan, cfg) = discover_now()?;
    let d = diff(&baseline, &scan);
    let report = ScanReport::from_diff(&d, &scan.errors, now_secs());

    let notifiers = build_notifiers(&cfg.notify);
    let errors = dispatch(&notifiers, &report);
    for e in &errors {
        eprintln!("skillshield: notifier failure: {e}");
    }

    if report.has_changes() {
        Ok(Code::CHANGES)
    } else {
        Ok(Code::OK)
    }
    .map_err(to_err::<String>) // no-op to keep signature uniform
}
```

Note: the trailing `.map_err` is unnecessary; write the ending simply as:

```rust
    Ok(if report.has_changes() { Code::CHANGES } else { Code::OK })
```

- [ ] **Step 3: Implement `commands/status.rs`**

```rust
use crate::commands::{discover_now, load_baseline_or_hint};
use crate::exit::Code;
use skillshield_core::diff::diff;
use skillshield_core::notify::render_text;
use skillshield_core::report::{now_secs, ScanReport};

pub fn run() -> Result<i32, String> {
    let baseline = load_baseline_or_hint()?;
    let (scan, _cfg) = discover_now()?;
    let d = diff(&baseline, &scan);
    let report = ScanReport::from_diff(&d, &scan.errors, now_secs());
    print!("{}", render_text(&report));
    Ok(if report.has_changes() { Code::CHANGES } else { Code::OK })
}
```

- [ ] **Step 4: Write an integration test**

Create `crates/skillshield-cli/tests/scan_flow.rs`:

```rust
// End-to-end: build a baseline, mutate, confirm `status` logic via core.
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::{Catalog, MatchSpec, Rule, Scope};
use skillshield_core::config::ScanConfig;
use skillshield_core::diff::{diff, ChangeKind};
use skillshield_core::discovery::discover;
use std::fs;

fn catalog_for(dir: &std::path::Path) -> Catalog {
    Catalog {
        rules: vec![Rule {
            id: "t".into(),
            description: "".into(),
            spec: MatchSpec::DirFileSet(format!("{}/", dir.display())),
            scope: Scope::Global,
        }],
    }
}

#[test]
fn detects_new_file_after_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("skills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.md"), "one").unwrap();

    let cat = catalog_for(&dir);
    let cfg = ScanConfig::default();
    let baseline = Baseline::new(discover(&cat, &cfg).entries);

    // A new file lands after baselining.
    fs::write(dir.join("evil.md"), "payload").unwrap();
    let scan2 = discover(&cat, &cfg);
    let d = diff(&baseline, &scan2);

    assert_eq!(d.findings.len(), 1);
    assert_eq!(d.findings[0].change, ChangeKind::Added);
    assert!(d.findings[0].path.ends_with("evil.md"));
}
```

Add `tempfile` to `skillshield-cli` `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p skillshield-cli`
Expected: PASS (cli + review_ui + integration).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(cli): read-only scan and status commands"
```

---

### Task 14: `review` and `trust` commands

**Files:**
- Modify: `crates/skillshield-cli/src/commands/review.rs`
- Modify: `crates/skillshield-cli/src/commands/trust.rs`
- Modify: `crates/skillshield-cli/src/commands/mod.rs` (add a baseline-write helper)

**Interfaces:**
- Consumes: `Baseline`, `diff`, `Finding`, `ChangeKind`, `Entry`, `discover_now`, `load_baseline_or_hint`.
- Produces:
  - `commands::save_baseline(baseline: &Baseline) -> Result<(), String>`.
  - `commands::apply_finding(baseline: &mut Baseline, scan: &Scan, path: &Path) -> bool` — for a given finding path, `upsert` the current entry (added/modified) or `remove_under` the exact path (removed); returns whether the baseline changed. Pure-ish; unit-tested.
  - `review::run()`: iterate findings, prompt accept/reject/skip per finding, apply accepted ones, save once at the end.
  - `trust::run(path)`: accept a single finding whose path equals the canonicalized `path`.

- [ ] **Step 1: Write failing test for `apply_finding` in `commands/mod.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use skillshield_core::baseline::Baseline;
    use skillshield_core::discovery::Scan;
    use skillshield_core::entry::{Entry, EntryKind};

    fn entry(path: &str, digest: &str) -> Entry {
        Entry {
            path: path.into(), kind: EntryKind::File, digest: Some(digest.into()),
            symlink_target: None, size: 1, mtime: 0, unhashed: false, source_rule: "r".into(),
        }
    }

    #[test]
    fn apply_added_upserts() {
        let mut b = Baseline::new(vec![]);
        let scan = Scan { entries: vec![entry("/x", "sha256:1")], errors: vec![] };
        let changed = apply_finding(&mut b, &scan, std::path::Path::new("/x"));
        assert!(changed);
        assert!(b.contains_path(std::path::Path::new("/x")));
    }

    #[test]
    fn apply_removed_deletes() {
        let mut b = Baseline::new(vec![entry("/gone", "sha256:1")]);
        let scan = Scan { entries: vec![], errors: vec![] };
        let changed = apply_finding(&mut b, &scan, std::path::Path::new("/gone"));
        assert!(changed);
        assert!(!b.contains_path(std::path::Path::new("/gone")));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p skillshield-cli apply_`
Expected: FAIL — `apply_finding` not found.

- [ ] **Step 3: Add helpers to `commands/mod.rs`**

```rust
use skillshield_core::baseline::Baseline;
use skillshield_core::discovery::Scan;

pub fn save_baseline(baseline: &Baseline) -> Result<(), String> {
    let path = skillshield_core::paths::baseline_path().map_err(to_err)?;
    baseline.save(&path).map_err(to_err)
}

/// Reconcile the baseline with the current scan for one path.
/// Returns true if the baseline changed.
pub fn apply_finding(baseline: &mut Baseline, scan: &Scan, path: &Path) -> bool {
    if let Some(entry) = scan.entries.iter().find(|e| e.path == path) {
        baseline.upsert(entry.clone());
        true
    } else if baseline.contains_path(path) {
        baseline.remove_under(path) > 0
    } else {
        false
    }
}
```

(Ensure `use std::path::Path;` remains at the top of the file.)

- [ ] **Step 4: Implement `commands/review.rs`**

```rust
use crate::commands::{apply_finding, discover_now, load_baseline_or_hint, save_baseline, to_err};
use crate::exit::Code;
use skillshield_core::diff::diff;
use std::io::{self, Write};

pub fn run() -> Result<i32, String> {
    let mut baseline = load_baseline_or_hint()?;
    let (scan, _cfg) = discover_now()?;
    let d = diff(&baseline, &scan);

    if d.findings.is_empty() {
        println!("No pending changes.");
        return Ok(Code::OK);
    }

    let mut changed = false;
    for f in &d.findings {
        print!(
            "{:?}  {}  [{}]  {}\n  Accept into baseline? [y/N/q] ",
            f.change, f.path.display(), f.rule_id, f.detail
        );
        io::stdout().flush().ok();
        let mut ans = String::new();
        io::stdin().read_line(&mut ans).map_err(to_err)?;
        match ans.trim() {
            "y" | "Y" | "yes" => {
                if apply_finding(&mut baseline, &scan, &f.path) {
                    changed = true;
                    println!("  accepted.");
                }
            }
            "q" | "Q" => break,
            _ => println!("  left as pending."),
        }
    }

    if changed {
        save_baseline(&baseline)?;
        println!("Baseline updated.");
    } else {
        println!("No changes accepted.");
    }
    Ok(Code::OK)
}
```

- [ ] **Step 5: Implement `commands/trust.rs`**

```rust
use crate::commands::{abs, apply_finding, discover_now, load_baseline_or_hint, save_baseline};
use crate::exit::Code;
use std::path::Path;

pub fn run(path: &Path) -> Result<i32, String> {
    let target = abs(path);
    let mut baseline = load_baseline_or_hint()?;
    let (scan, _cfg) = discover_now()?;
    if apply_finding(&mut baseline, &scan, &target) {
        save_baseline(&baseline)?;
        println!("Trusted {}", target.display());
        Ok(Code::OK)
    } else {
        Err(format!(
            "{} is not a pending finding (unchanged, or not under a monitored location).",
            target.display()
        ))
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p skillshield-cli`
Expected: PASS (apply_finding tests + prior).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(cli): interactive review and scriptable trust"
```

---

### Task 15: `monitor` and `unmonitor` commands

**Files:**
- Modify: `crates/skillshield-cli/src/commands/monitor.rs`
- Modify: `crates/skillshield-cli/src/commands/mod.rs` (config write helper, if not already present from Task 12)

**Interfaces:**
- Consumes: `Config`, `Catalog`, `discover`, `Baseline`, `paths`, `abs`.
- Produces:
  - `commands::write_config(cfg: &Config) -> Result<(), String>` — atomic 0600 write to `paths::config_path()`. (Refactor the `write_config` from Task 12's `init.rs` into `commands/mod.rs` and have `init.rs` call it, to avoid duplication — DRY.)
  - `monitor::run(path)`: canonicalize `path`; if not already in `cfg.scan.project_roots`, add it and `write_config`; then discover, and `upsert` every discovered entry under that root into the baseline; save baseline.
  - `monitor::run_unmonitor(path)`: remove the canonicalized `path` from `cfg.scan.project_roots`, `write_config`, then `remove_under(path)` from the baseline and save.

- [ ] **Step 1: Refactor `write_config` into `commands/mod.rs`**

Move the `write_config` fn from `commands/init.rs` (Task 12 Step 4) into `commands/mod.rs`, changing its signature to take only the config and resolve the path internally:

```rust
pub fn write_config(cfg: &skillshield_core::config::Config) -> Result<(), String> {
    use std::io::Write;
    let path = skillshield_core::paths::config_path().map_err(to_err)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(to_err)?;
    }
    let text = toml::to_string_pretty(cfg).map_err(to_err)?;
    let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(to_err)?;
    tmp.write_all(text.as_bytes()).map_err(to_err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600)).map_err(to_err)?;
    }
    tmp.persist(&path).map_err(|e| to_err(e.error))?;
    Ok(())
}
```

Update `init.rs`'s `maybe_setup_desktop` to call `crate::commands::write_config(&cfg)` and delete the local `write_config` there.

- [ ] **Step 2: Implement `commands/monitor.rs`**

```rust
use crate::commands::{abs, discover_now, to_err, write_config};
use crate::exit::Code;
use skillshield_core::baseline::Baseline;
use skillshield_core::config::Config;
use skillshield_core::paths;
use std::path::Path;

pub fn run(path: &Path) -> Result<i32, String> {
    let target = abs(path);
    if !target.is_dir() {
        return Err(format!("{} is not a directory", target.display()));
    }
    let mut cfg = Config::load().map_err(to_err)?;
    let target_str = target.to_string_lossy().to_string();
    if !cfg.scan.project_roots.iter().any(|r| r == &target_str) {
        cfg.scan.project_roots.push(target_str.clone());
        write_config(&cfg)?;
        println!("Added project root {} to config.", target.display());
    } else {
        println!("{} already monitored; refreshing baseline.", target.display());
    }

    // Discover with the updated config, trust everything under this root.
    let (scan, _cfg) = discover_now()?;
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    let mut baseline = if baseline_path.exists() {
        Baseline::load(&baseline_path).map_err(to_err)?
    } else {
        Baseline::new(vec![])
    };
    let mut added = 0;
    for e in scan.entries.iter().filter(|e| e.path.starts_with(&target)) {
        baseline.upsert(e.clone());
        added += 1;
    }
    baseline.save(&baseline_path).map_err(to_err)?;
    println!("Trusted {added} file(s) under {}.", target.display());
    Ok(Code::OK)
}

pub fn run_unmonitor(path: &Path) -> Result<i32, String> {
    let target = abs(path);
    let mut cfg = Config::load().map_err(to_err)?;
    let target_str = target.to_string_lossy().to_string();
    let before = cfg.scan.project_roots.len();
    cfg.scan.project_roots.retain(|r| r != &target_str);
    if cfg.scan.project_roots.len() == before {
        return Err(format!("{} is not a monitored project root.", target.display()));
    }
    write_config(&cfg)?;

    let baseline_path = paths::baseline_path().map_err(to_err)?;
    if baseline_path.exists() {
        let mut baseline = Baseline::load(&baseline_path).map_err(to_err)?;
        let removed = baseline.remove_under(&target);
        baseline.save(&baseline_path).map_err(to_err)?;
        println!("Removed {} and pruned {removed} baseline entry/entries.", target.display());
    } else {
        println!("Removed {} from config (no baseline to prune).", target.display());
    }
    Ok(Code::OK)
}
```

- [ ] **Step 3: Write an integration test**

Create `crates/skillshield-cli/tests/monitor_flow.rs`:

```rust
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::Catalog;
use skillshield_core::config::ScanConfig;
use skillshield_core::discovery::discover;
use std::fs;

#[test]
fn monitor_root_picks_up_project_agents_md() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();
    fs::write(proj.join("AGENTS.md"), "rules").unwrap();

    let catalog = Catalog::builtin();
    let cfg = ScanConfig {
        project_roots: vec![proj.to_string_lossy().to_string()],
        ..ScanConfig::default()
    };
    let scan = discover(&catalog, &cfg);
    let baseline = Baseline::new(scan.entries);
    assert!(baseline.contains_path(&proj.join("AGENTS.md")));
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p skillshield-cli`
Expected: PASS (all cli tests + integration).

- [ ] **Step 5: Full workspace test**

Run: `cargo test`
Expected: PASS (all core + cli tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(cli): monitor/unmonitor project roots"
```

---

### Task 16: Packaging, docs, and end-to-end smoke

**Files:**
- Create: `packaging/skillshield.service`
- Create: `packaging/skillshield.timer`
- Create: `packaging/README.md`
- Create: `README.md` (repo root)
- Create: `crates/skillshield-cli/tests/cli_e2e.rs`

**Interfaces:**
- Consumes: the built `skillshield` binary.
- Produces: install artifacts and a documented usage flow; one end-to-end test driving the compiled binary with an isolated `HOME`/XDG.

- [ ] **Step 1: Create `packaging/skillshield.service`**

```ini
[Unit]
Description=SkillShield scan for AI-agent config changes

[Service]
Type=oneshot
ExecStart=%h/.cargo/bin/skillshield scan
# Exit code 10 means "changes detected" — treat as success for the timer.
SuccessExitStatus=10
```

- [ ] **Step 2: Create `packaging/skillshield.timer`**

```ini
[Unit]
Description=Run SkillShield daily

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

- [ ] **Step 3: Create `packaging/README.md`**

````markdown
# Packaging & scheduling

## Systemd (user-level)

Copy the units into your user unit directory and enable the timer:

```bash
mkdir -p ~/.config/systemd/user
cp skillshield.service skillshield.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now skillshield.timer
```

Check results with `journalctl --user -u skillshield.service` or read
`~/.local/share/skillshield/last-report.json`.

## Cron alternative

```cron
# Daily at 09:00; exit code 10 (changes) is fine, cron only cares about run.
0 9 * * * /home/youruser/.cargo/bin/skillshield scan >> ~/.local/share/skillshield/cron.log 2>&1
```

Neither is installed automatically — `skillshield init` prints these hints.
````

- [ ] **Step 4: Create repo-root `README.md`**

```markdown
# SkillShield

A Linux tripwire for the files AI coding agents consume — skills, plugins,
`CLAUDE.md`/`AGENTS.md`, MCP configs, and more. It baselines what exists, then
warns you when anything is added, modified, or removed. Detect-and-warn only:
it never edits or blocks your files.

## Install

```bash
cargo install --path crates/skillshield-cli
```

## Quick start

```bash
skillshield init                 # discover artifacts, review, write baseline
skillshield monitor ~/projects/x # add a project directory to watch
skillshield scan                 # check for changes (exit 10 if any)
skillshield status               # human-readable diff
skillshield review               # accept/reject pending changes
```

Config: `~/.config/skillshield/config.toml`.
State: `~/.local/share/skillshield/{baseline.json,last-report.json}`.

See `packaging/` for Systemd/cron scheduling.
```

- [ ] **Step 5: Write the end-to-end binary test**

Create `crates/skillshield-cli/tests/cli_e2e.rs`:

```rust
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_skillshield")
}

#[test]
fn init_then_scan_detects_change() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    // Seed a fake global artifact under a fake HOME.
    std::fs::create_dir_all(home.join(".claude/skills/a")).unwrap();
    std::fs::write(home.join(".claude/skills/a/SKILL.md"), "one").unwrap();

    let envs = [
        ("HOME", home.to_str().unwrap()),
        ("XDG_CONFIG_HOME", home.join(".config").to_str().unwrap()),
        ("XDG_DATA_HOME", home.join(".local/share").to_str().unwrap()),
    ];

    // init (auto-trust via piped "y")
    let mut init = Command::new(bin())
        .arg("init")
        .envs(envs)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    use std::io::Write;
    init.stdin.as_mut().unwrap().write_all(b"y\n").unwrap();
    assert!(init.wait().unwrap().success());

    // scan: no changes yet → exit 0
    let status = Command::new(bin()).arg("scan").envs(envs).status().unwrap();
    assert_eq!(status.code(), Some(0));

    // introduce a new file → exit 10
    std::fs::write(home.join(".claude/skills/a/EVIL.md"), "payload").unwrap();
    let status = Command::new(bin()).arg("scan").envs(envs).status().unwrap();
    assert_eq!(status.code(), Some(10));
}
```

Note: this test relies on `HOME`/XDG overrides. `directories` honors `XDG_CONFIG_HOME`/`XDG_DATA_HOME`; the catalog's `~` expansion uses `HOME`. Ensure `expand_tilde`/`home_dir` resolve via `$HOME` (the `directories::BaseDirs` home honors `$HOME` on Linux).

- [ ] **Step 6: Run the whole suite**

Run: `cargo test`
Expected: PASS (all unit + integration + e2e tests).

- [ ] **Step 7: Build a release binary as a final check**

Run: `cargo build --release`
Expected: compiles cleanly with no errors.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "docs: packaging, README, and end-to-end CLI test"
```

---

## Notes for the implementer

- Work top-to-bottom; each task's `Interfaces` block names the exact types/functions later tasks rely on.
- Run `cargo test` from the workspace root after every task.
- Keep `scan`/`status` read-only against the baseline — this is a security invariant, not a nicety.
- If a `clippy` lint fires (`cargo clippy --workspace`), fix it before committing; treat warnings as errors for the notify/discovery modules where correctness matters most.
