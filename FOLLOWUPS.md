# Follow-ups

No open items — all recorded follow-ups have been resolved (see below).

## Resolved

- ~~Invalid user globs silently swallowed~~ — `discovery::validate_globs` now records a `ScanError` for any invalid `scan.ignore` / catalog glob pattern (fail-loud).
- ~~Pre-existing clippy lints (`derivable_impls` in `config.rs`, `cmp_owned` in a `diff.rs` test)~~ — fixed; CI now enforces `clippy -D warnings` and `fmt --check`.
- ~~Symlinked `$HOME` breaks `trust`/`monitor` path matching~~ — `paths::normalize` (used by `commands::abs`) now normalizes without resolving symlinks, so a path as printed by `scan`/`status` matches the stored entry. Was: `canonicalize` resolved symlink prefixes and diverged from discovery's `expand_tilde` paths.
- ~~Non-UTF8 baseline surfaces as `Io` rather than `Corrupt`~~ — `Baseline::load` now reads raw bytes so a genuine I/O failure stays `Io`, while invalid UTF-8 (damaged/tampered content) is classified `Corrupt`. Covered by a test.
- ~~`expand_tilde` silent `NoHome` fallback~~ — reviewed and kept intentional (infallible by design; `NoHome` is unreachable where `$HOME` is always set; a literal `~` path matches nothing, which is safe). Documented on the function.
- ~~Missing module docs / `path.parent()` duplication~~ — added `//!` docs to the core modules; deduplicated the double `path.parent()` in `Baseline::save` and `ReportFileNotifier::notify`.
- ~~Duplicated `"SkillShield: N change(s)"` subject string~~ — extracted `notify::change_subject`, used by the desktop and email channels.
- ~~`ScanDiff` derives unused `Default`~~ — removed the derive (no call site used it).
- ~~`Notifier::id()` only used in tests~~ — removed from the trait; it was unused in non-test code and redundant with `NotifyError.channel`.
