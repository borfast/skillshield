# SkillShield — Project Status & Handoff

_Snapshot for resuming work in a future session. Last updated: 2026-07-16._

## Snapshot

- **What it is:** a Linux CLI tripwire (Rust) that baselines the files AI coding
  agents load — skills, plugins, `CLAUDE.md`/`AGENTS.md`, MCP configs, hooks,
  settings — and warns when any of them are added, modified, or removed.
  **Detect-and-warn only**: it never edits or blocks files.
- **Repo:** https://github.com/borfast/skillshield — branch `main`, latest
  commit `bb8b0cf`. Working tree clean, in sync with origin.
- **State:** feature-complete for the intended scope; MIT-licensed; CI green
  (GitHub Actions: `fmt --check`, `clippy -D warnings`, `test`, release build).
  **72 tests** passing; `cargo build --release` clean.
- **Workflow convention:** each change was built on a short feature branch,
  reviewed (subagent code review for substantial work), then fast-forward
  merged to `main` and pushed; CI watched to green. Commit/push only when the
  user asks. Stage explicit paths (never `git add -A` — it twice swept in
  unrelated working-tree files).

## Architecture

Cargo workspace, library/binary split:

- **`crates/skillshield-core`** — the engine (no CLI concerns):
  - `catalog.rs` — `Rule` (id, `group`, description, `MatchSpec`, `Scope`);
    `MatchSpec` = `ExactPath | Glob | DirFileSet`; the built-in `default_rules()`
    organized into per-agent **groups**; `Catalog::{builtin, apply, with_profiles,
    retain_groups}`; `global_groups()`/`group_default_on()`; `agent_default_root`,
    `profileable_agents`, `profile_name`.
  - `discovery.rs` — walks global rules + opted-in project roots → `Scan`
    (`entries` + `errors`). Never traverses into symlinked dirs; global `Glob`
    rules recurse only when the pattern contains `**` (`glob_literal_base`);
    `validate_globs` fails loud on bad user globs.
  - `hashing.rs` — streamed SHA-256; `hash_file` + `hash_symlink_target`
    (one-hop, size-guarded).
  - `baseline.rs` — trusted snapshot (JSON, atomic 0600 write, integrity digest;
    corrupt/non-UTF8 → `Error::Corrupt`, never silent reset).
  - `diff.rs` — `changed()` compares kind → digest → symlink target → unhashed;
    `ScanDiff` of Added/Modified/Removed `Finding`s.
  - `config.rs` — XDG TOML config with defaults; `Profile { agent, path }`.
  - `report.rs`, `entry.rs`, `error.rs`, `paths.rs` (`normalize()` = symlink-free
    absolutize; `expand_tilde`).
  - `notify/` — `Notifier` trait + `dispatch` (failure-isolated); channels:
    `report_file`, `stdout`, `desktop`, `email` (sendmail or SMTP via lettre),
    `webhook`. Alert channels (desktop/email/webhook) stay quiet on a clean run;
    `report`/`stdout` always fire.
- **`crates/skillshield-cli`** — thin `clap` binary `skillshield`:
  `commands/{init,scan,status,config,review,trust,monitor,profile,schedule}.rs`
  + `commands/mod.rs` (shared helpers: `discover_now`, `load_baseline_or_hint`,
  `write_config`, `ensure_config_file`, `abs`, `apply_finding`), `cli.rs`,
  `exit.rs` (codes: 0 none, 10 changes, 1 error), `review_ui.rs`.

## Commands

`init`, `scan [-v]`, `status`, `config`, `review`, `trust <path>`,
`monitor <path>` (project root), `forget <path>`, `add-profile <agent> <path>
[--remove]`, `schedule [--remove|--systemd|--cron|--yes|--interval|--time]`.

- Only `init`/`review`/`trust`/`monitor`/`forget`/`add-profile` write the
  baseline. `scan`/`status` are strictly read-only against it.
- `scan` always prints a one-line result; `-v` lists every item checked.
- `init`: creates the config if missing, selects the **recommended** groups
  (default-on + present), persists them to `[catalog].monitor`, and tells the
  user to edit that list + re-run `init --force` to change it. Preserves an
  existing `monitor` selection on re-run. No interactive picker (see below).

## Config & state

- Config: `$XDG_CONFIG_HOME/skillshield/config.toml` (materialized with defaults
  on first `init`/`config`). State: `$XDG_DATA_HOME/skillshield/{baseline.json,
  last-report.json, skillshield.log}`.
- Shape: `[scan]` (follow_symlinks, max_hash_bytes, project_roots,
  project_max_depth, ignore); `[catalog]` (extra_files, disable, `monitor` =
  allowlist of group keys, `profiles`); `[[catalog.profiles]]` (agent, path);
  `[notify]` (channels, `[notify.email]`, `[notify.webhook]`).

## Catalog groups & profiles (the core model)

- Built-in global groups (default-on except `claude.memory`): `claude.core`,
  `claude.config`, `claude.memory`, `codex.core`, `codex.config`, `gemini`,
  `cursor`, `copilot`. Rules target **behavior files only** — NOT whole agent
  home dirs (those are mostly churny runtime state: sandbox/sessions/cache/logs).
- `[catalog].monitor` is an allowlist of group keys; `None`/absent = all.
- **Profiles**: `add-profile claude ~/.claude-gc` re-roots that agent's
  within-home rules at the path, namespaced by the **full path** (e.g.
  `claude.core@/home/u/.claude-gc`) so same-basename profiles don't collide;
  the path is normalized to absolute (CWD-safe). Supports claude/codex/gemini.

## Key design decisions (the "why")

- **Detect-and-warn only** (tripwire), not quarantine/blocking.
- **Narrow catalog, not whole-home** — monitoring `~/.claude/` etc. wholesale
  pulled in hundreds of churny state files → alert fatigue. Target loaded
  behavior files; users pick groups.
- **Fail loud** — unreadable dirs, invalid globs, corrupt/tampered baseline all
  surface; never swallowed or silently reset.
- **One-hop symlink hashing** + explicit file↔symlink type-flip detection.
- **Quiet-by-default scan for alert channels** (webhook/desktop/email), so a
  scheduled hourly run doesn't spam; stdout/report still record every run.
- **No TUI** — a Ratatui checkbox picker was built and then removed at the
  user's request in favor of the simpler defaults + edit-`[catalog].monitor`
  model. Do not reintroduce a TUI dependency without asking.

## Build / test / CI

- `cargo test` (workspace), `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all --check`, `cargo build --release`. All must pass;
  CI enforces the same on push/PR to `main` (`.github/workflows/ci.yml`).
- Pure logic is unit-tested; system-interaction shells (systemctl/crontab/
  notify-rust) are thin and exercised via manual/live smokes, not unit tests.

## Open items / caveats

- See `FOLLOWUPS.md` (all currently resolved; keep it updated).
- **Not driven on a real TTY here** was a caveat for the (now-removed) picker
  and desktop notifications — this session's environment is headless. Desktop
  notification and any future interactive path should be sanity-checked on a
  real desktop.
- **Codex plugin assets** (LICENSE/README/PNG under `~/.codex/plugins/cache/`)
  are intentionally monitored — a tripwire wants to notice any new file in a
  plugin. The user chose to keep them.
- **cursor/copilot** catalog rules are best-effort on exact file layout; the
  copilot group was narrowed to `**/*instructions*.md` + `**/mcp.json` after it
  was found grabbing ~450 session/auth files.

## Design records

- Spec: `docs/superpowers/specs/2026-07-09-skillshield-design.md`
- Implementation plan (initial 16-task build): `docs/superpowers/plans/2026-07-09-skillshield.md`
- Everything after the initial build is captured in the git history (see the
  commit list) and this document.
