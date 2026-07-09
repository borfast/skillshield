# SkillShield — Design Spec

**Date:** 2026-07-09
**Status:** Approved for planning

## 1. Purpose

SkillShield is a tripwire for the files and directories that AI coding agents
consume — skills, plugins, marketplaces, `AGENTS.md`, `CLAUDE.md`, MCP configs,
and similar. It does **not** attempt to classify content as malicious. Instead,
it establishes a trusted baseline of what exists, then periodically re-scans and
**warns the user about anything new, changed, or removed**, so a malicious or
unexpected change (e.g. a new skill sliding into `~/.claude/skills/`) can't take
effect unnoticed.

Target platform for now: **Linux**, using XDG base directories with sane
fallbacks. macOS/Windows are explicitly out of scope for the first version but
the design should not preclude them.

Intended to be published as **open source**, so no user-specific or
machine-specific paths are baked into defaults.

## 2. Core principles

- **Detect & warn only (tripwire).** No quarantine, no auto-blocking. Purely
  observational; the user decides what to do.
- **Fail loud, never silent.** This is a security tool. Unreadable locations,
  corrupt baselines, and operational errors are surfaced, never swallowed.
- **The automated scan is strictly read-only against trust state.** Only
  explicit/interactive commands ever mutate the baseline, so a malicious change
  can never auto-trust itself.
- **Nothing user-specific in defaults.** Ships vendor-defined global locations
  and project *filename patterns* only; users opt their own project directories
  in explicitly.
- **YAGNI.** Periodic model now; real-time daemon deferred but not designed out.

## 3. Execution model

Periodic, stateless CLI: the scan/diff engine lives in a library crate so a
future real-time `daemon`/`watch` mode (inotify) is additive, not a rewrite. A
Systemd timer or cron runs `skillshield scan` on a schedule; each run loads
the baseline, walks the filesystem, diffs, notifies, and exits. Nothing runs
while idle.

## 4. Architecture & crate layout

Cargo workspace with a library/binary split:

```
skillshield/
├── crates/
│   ├── skillshield-core/     # library: scan → diff → notify pipeline
│   │   ├── catalog           # what to look for + built-in default catalog
│   │   ├── discovery         # walk global locations + crawl opted-in project roots
│   │   ├── hashing           # per-file digest + entry metadata
│   │   ├── baseline          # load/save the trusted snapshot ("whitelist")
│   │   ├── diff              # compare current scan vs baseline → ScanDiff
│   │   ├── config            # XDG-aware config loading
│   │   └── notify            # Notifier trait + built-in channels
│   └── skillshield-cli/      # binary: subcommands, interactive review, wiring
├── docs/superpowers/specs/
└── packaging/                # systemd unit + timer, cron example
```

Each module has one job and a narrow interface. `diff` takes
`(baseline, current_scan)` and returns a `ScanDiff` — it does not touch the
filesystem or notify anyone. The CLI is a thin shell over the core.

## 5. Data model & state

### 5.1 Monitored entry

One record per tracked file:

```
Entry {
  path:        absolute, canonicalized (without following symlinks)
  kind:        File | Symlink
  digest:      "sha256:…"        // content hash: regular files, AND the resolved
                                 // content behind a symlink to a regular file
  symlink_tgt: Option<String>    // literal target if it's a symlink
  size, mtime                    // display / cheap change hints, NOT trust signals
  source:      catalog rule id + which root matched it
  trusted_at:  timestamp added to baseline
}
```

### 5.2 Directories

Directories are **not** hashed. Instead the *set* of files within a monitored
directory is part of the baseline, so a **new file appearing** in a monitored
directory is an `Added` finding even when nothing existing changed. This is the
core threat: a new artifact sliding in unnoticed.

### 5.3 Symlinks

- The **walk** never traverses *into* symlinked directories (prevents escaping
  monitored roots and loops).
- The symlink's literal target string is recorded. Retargeting → `Modified`.
- **One-hop content hashing:** for a symlink that resolves to a *regular file*,
  the resolved file's content is hashed (a single dereference via the OS, which
  may follow a chain of links to a final file), and that digest is stored on the
  symlink entry *in addition to* the literal target. This catches a **content
  swap behind a stable symlink** — the exact attack the tool exists to detect,
  since an agent following the link reads the current target contents.
  - Guarded by `max_hash_bytes` (oversized target → `unhashed`).
  - A symlink to a **directory**, a special file, or a **dangling** target is
    *not* traversed/hashed; only its literal target is recorded (`digest = None`).

### 5.4 File ↔ symlink type flips

A monitored path changing `kind` (regular file ↔ symlink) is always a `Modified`
finding: the `diff` compares `kind` explicitly, so a flip is reported even if
content digests happen to coincide. The finding detail states the direction
(e.g. `type changed (File -> Symlink)`).

### 5.5 Baseline

- Single JSON file at `$XDG_DATA_HOME/skillshield/baseline.json` (fallback
  `~/.local/share/skillshield/baseline.json`).
- JSON: human-inspectable, diff-friendly, serde-trivial.
- Top-level `version` field for forward migration.
- Written **atomically** (temp file + rename); an interrupted run never corrupts
  it. Created with `0600` permissions.
- Stores a top-level digest over its own entries so **tampering with the
  baseline itself is detectable** and reported rather than silently trusted.

### 5.6 Hashing

- SHA-256, **streamed** (never slurp whole files into memory).
- Configurable `max_hash_bytes` guard: oversized files are recorded by
  size+mtime and flagged **"unhashed"** — never skipped silently.

## 6. Config & catalog

### 6.1 Config file

`$XDG_CONFIG_HOME/skillshield/config.toml` (fallback `~/.config/…`), TOML for
hand-editing. Every field has a built-in default; the tool runs with no config.

```toml
[scan]
follow_symlinks   = false
max_hash_bytes    = 5_000_000
project_roots     = []          # user opt-in only; empty by default
project_max_depth = 6           # bound the crawl
ignore = ["**/node_modules/**", "**/.git/**", "**/target/**", "**/vendor/**"]

[catalog]
extra_files = []                # additional filenames/globs to treat as artifacts
disable     = []                # built-in rule ids to turn off

[notify]
channels = ["report", "stdout"] # desktop added automatically by `init` if a GUI session is detected
# per-channel tables, e.g. [notify.email], [notify.webhook] …
```

### 6.2 Built-in catalog (curated defaults)

Each rule has a stable `id`, description, and a match spec: `ExactPath`,
`Glob`, or `DirFileSet` (track the set of files in a directory). Shipping the
catalog as data means adding coverage for a new agent is a catalog edit, not a
code change. `~` expands to the user's home dir.

**Global locations** (scanned directly, always on unless disabled):

| id | Match | Notes |
|----|-------|-------|
| `claude.home`            | `DirFileSet ~/.claude/`               | top-level files (settings.json, etc.) |
| `claude.skills`          | `DirFileSet ~/.claude/skills/`        | recursive |
| `claude.plugins`         | `DirFileSet ~/.claude/plugins/`       | recursive; includes marketplaces cache |
| `claude.commands`        | `DirFileSet ~/.claude/commands/`      | recursive |
| `claude.agents`          | `DirFileSet ~/.claude/agents/`        | recursive |
| `claude.md.home`         | `ExactPath ~/.claude/CLAUDE.md`       | global memory |
| `claude.settings`        | `Glob ~/.claude/settings*.json`       | settings + local overrides |
| `claude.mcp`             | `ExactPath ~/.claude.json`            | MCP / project registry |
| `claude.config.xdg`      | `DirFileSet ~/.config/claude/`        | recursive |
| `codex.home`             | `DirFileSet ~/.codex/`                | recursive |
| `codex.config.xdg`       | `DirFileSet ~/.config/codex/`         | recursive |
| `gemini.home`            | `DirFileSet ~/.gemini/`               | recursive |
| `gemini.md.home`         | `ExactPath ~/.gemini/GEMINI.md`       | |
| `gemini.config.xdg`      | `DirFileSet ~/.config/gemini/`        | recursive |
| `cursor.home`            | `DirFileSet ~/.cursor/`               | recursive; rules, MCP |
| `copilot.config.xdg`     | `DirFileSet ~/.config/github-copilot/`| recursive |
| `mcp.config.xdg`         | `DirFileSet ~/.config/mcp/`           | recursive |

**Project artifact patterns** (matched only under user-opted `project_roots`,
respecting `project_max_depth` and `ignore`):

| id | Match (relative to a project root) | Notes |
|----|-----|-------|
| `proj.claude.md`      | `Glob **/CLAUDE.md`        | |
| `proj.claude.local`   | `Glob **/CLAUDE.local.md`  | |
| `proj.agents.md`      | `Glob **/AGENTS.md`        | |
| `proj.gemini.md`      | `Glob **/GEMINI.md`        | |
| `proj.claude.dir`     | `DirFileSet **/.claude/`   | skills/commands/agents/settings in-project |
| `proj.cursor.dir`     | `DirFileSet **/.cursor/`   | rules |
| `proj.cursorrules`    | `Glob **/.cursorrules`     | legacy single-file |
| `proj.mcp.json`       | `Glob **/.mcp.json`        | project MCP servers |
| `proj.github.copilot` | `Glob **/.github/copilot-instructions.md` | |

This list is a curated starting point and is expected to grow via catalog edits
and user `extra_files`.

### 6.3 Discovery

- Global locations walked directly. Missing locations are absent, not errors.
- Each `project_root` crawled with `project_max_depth` + `ignore`; unreadable
  dirs are recorded as errors and reported, not silently skipped.
- Only paths the user explicitly opted in are ever crawled — no broad home-dir
  sweep.

## 7. CLI commands & workflows

```
skillshield init             # first-run: discover → grouped review → write baseline
skillshield scan             # scheduled: discover → diff → notify → exit
skillshield status           # show current diff vs baseline; read-only
skillshield review           # interactively resolve pending findings (accept/reject)
skillshield monitor <path>   # add a project root: crawl once, record in config, trust findings
skillshield trust <path>     # accept a specific finding into the baseline (scriptable)
skillshield unmonitor <path> # remove a project root from config; prune its baseline entries
```

### 7.1 Exit codes (`scan`, `status`)

- `0` — no changes
- `10` — changes detected
- other non-zero — operational error (broken run)

Lets scheduling/scripts distinguish "found something" from "tool broke."

### 7.2 First run (`init`)

Discover everything, present a **grouped review UI**: findings grouped by
location/root (e.g. "`~/.claude/skills`: 12 files"). The user can trust a whole
group, drill into a group to inspect individual files, or trust-all. Baseline is
then written.

- Detects a graphical session (`$DISPLAY`/`$WAYLAND_DISPLAY` present and
  `notify-send` available); if found, adds `desktop` to `notify.channels` in the
  generated config and sends a one-off test notification to confirm it works. If
  not found, leaves it out and prints how to enable it later.
- Idempotent guard: re-running `init` with an existing baseline refuses unless
  `--force`, directing the user to `scan`/`review`.

### 7.3 Scheduled (`scan`)

Non-interactive. Detects, notifies via configured channels, exits `10` if
anything changed. **Never mutates the baseline.**

### 7.4 Investigate & resolve

On alert: `skillshield status` for details, then `skillshield review`
(interactive) or `skillshield trust <path>` to accept legitimate changes.
Rejecting a finding leaves the baseline unchanged, so warnings persist until
dealt with.

### 7.5 Baseline-write invariant

Only `init`, `review`, `trust`, `monitor`, and `unmonitor` write the baseline.
`scan` and `status` are strictly read-only against trust state.

## 8. Notifier architecture

```rust
trait Notifier {
    fn id(&self) -> &str;
    fn notify(&self, report: &ScanReport) -> Result<(), NotifyError>;
}
```

`ScanReport` is a structured summary: counts plus per-finding path, kind,
change type (added/modified/removed), catalog rule, and old→new digest. Each
channel renders it to suit its medium.

**Built-in channels (now):**

- `report` — writes structured report to
  `$XDG_DATA_HOME/skillshield/last-report.json` plus a human-readable log.
  Always-available, headless.
- `stdout` — prints a summary; pairs with the exit code.
- `desktop` — `notify-send`/libnotify; degrades gracefully (logs a warning) with
  no graphical session.
- `email` — local `sendmail` or SMTP; opt-in, own config table.
- `webhook` — POSTs the JSON report to a configurable URL with configurable
  headers; covers ntfy/Slack/Telegram/Discord for most users without
  service-specific code.

**Selection & extensibility:** channels selected by id from `notify.channels`,
each with its own `[notify.<id>]` table. **Static registry** (`match` over known
ids) — no dynamic plugin loading. Adding a channel = implement the trait +
register it.

**Failure isolation:** notifiers run independently; one failing (e.g. SMTP down)
logs an error but doesn't abort others or fail the scan. Exit code reflects
*findings*, not notifier health; notifier failures surface as non-fatal warnings
in the log.

## 9. Error handling & security hardening

**Error philosophy — fail loud:**

- Discovery errors (unreadable dir, permission denied) collected and reported in
  scan output — an unreadable location is security-relevant signal.
- Corrupt/tampered baseline is a hard error that halts and alerts; never "start
  fresh and silently re-trust."
- Operational errors exit non-zero (non-10) so scheduling never mistakes a
  broken run for a clean one.

**Security-specific:**

- No symlink following; symlink targets tracked as content.
- Baseline & config written atomically (temp + rename), `0600`, under
  user-owned XDG dirs.
- Scan strictly read-only against trust state.
- Path canonicalization to absolute form to prevent `..`/relative-path evasion,
  without traversing symlinks.

## 10. Testing

- **Unit:** `diff` (added/modified/removed/symlink-retarget permutations),
  catalog matching, config precedence, baseline serialize/round-trip +
  migration, hashing (including oversized/unhashed path).
- **Integration:** temp fixture tree → `init` → mutate files
  (add/modify/delete/retarget symlink) → assert `scan` reports exactly the right
  findings and exit codes. A fake in-memory notifier captures `ScanReport`s for
  assertions.
- TDD throughout (test-driven-development skill during implementation).

## 11. Packaging & scheduling (`packaging/`)

- `skillshield.service` (oneshot) + `skillshield.timer` (user-level, e.g.
  daily), with `systemctl --user enable --now` instructions.
- Documented cron one-liner alternative.
- Neither auto-installed; `init` prints the exact commands so the user opts in.

## 12. Out of scope (for now)

- Real-time daemon / inotify watch mode (designed for as an additive future
  mode).
- macOS / Windows support.
- Content classification / maliciousness detection.
- Quarantine or active blocking.
