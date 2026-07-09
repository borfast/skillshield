# Follow-ups

Deferred items, recorded so they aren't lost.

- A symlinked `$HOME` makes `trust`/`monitor` path matching fail; discovery paths should be canonicalized (or compared canonicalized) so matching is robust to symlinks.
- A non-UTF8 baseline file currently surfaces as an `Io` error rather than a `Corrupt` one; error classification should be revisited.
- `expand_tilde` silently falls back on a missing `HOME` (`NoHome`) instead of surfacing the condition explicitly.
- Missing module docs and cosmetic helper-merge opportunities (e.g. `path.parent()` deduplication, a shared "subject string" helper) were identified but intentionally left alone as cosmetic-only.
- The `"SkillShield: N change(s)"` subject string is duplicated between the email and desktop notifiers; consider extracting a shared helper.
- `ScanDiff` derives `Default`, which does not appear to be requested/used anywhere; consider removing the derive if confirmed unused.
- `Notifier::id()` is currently only exercised by tests; confirm whether it's meant for future use (e.g. config-driven notifier selection) or can be dropped.

## Resolved

- ~~Invalid user globs silently swallowed~~ — `discovery::validate_globs` now records a `ScanError` for any invalid `scan.ignore` / catalog glob pattern (fail-loud).
- ~~Pre-existing clippy lints (`derivable_impls` in `config.rs`, `cmp_owned` in a `diff.rs` test)~~ — fixed; CI now enforces `clippy -D warnings` and `fmt --check`.
