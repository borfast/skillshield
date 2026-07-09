# Follow-ups

Deferred items from the finish-line cleanup pass (not fixed in this pass, recorded so they aren't lost):

- Invalid user globs are silently swallowed; should push a `ScanError` instead (fail-loud). Needs a deliberate design choice about where to validate user-supplied globs.
- A symlinked `$HOME` makes `trust`/`monitor` path matching fail; discovery paths should be canonicalized (or compared canonicalized) so matching is robust to symlinks.
- A non-UTF8 baseline file currently surfaces as an `Io` error rather than a `Corrupt` one; error classification should be revisited.
- `expand_tilde` silently falls back on a missing `HOME` (`NoHome`) instead of surfacing the condition explicitly.
- Missing module docs and cosmetic helper-merge opportunities (e.g. `path.parent()` deduplication, a shared "subject string" helper) were identified but intentionally left alone as cosmetic-only.
- Pre-existing clippy lints left as-is: `derivable_impls` in `config.rs`, `cmp_owned` in a `diff.rs` test.
- The `"SkillShield: N change(s)"` subject string is duplicated between the email and desktop notifiers; consider extracting a shared helper.
- `ScanDiff` derives `Default`, which does not appear to be requested/used anywhere; consider removing the derive if confirmed unused.
- `Notifier::id()` is currently only exercised by tests; confirm whether it's meant for future use (e.g. config-driven notifier selection) or can be dropped.
