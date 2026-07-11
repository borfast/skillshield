use crate::commands::{discover_now, load_baseline_or_hint};
use crate::exit::Code;
use skillshield_core::diff::{diff, ChangeKind, ScanDiff};
use skillshield_core::entry::Entry;
use skillshield_core::notify::{build_notifiers, dispatch};
use skillshield_core::report::{now_secs, ScanReport};

pub fn run(verbose: bool) -> Result<i32, String> {
    let baseline = load_baseline_or_hint()?;
    let (scan, cfg) = discover_now()?;
    let d = diff(&baseline, &scan);
    let report = ScanReport::from_diff(&d, &scan.errors, now_secs());

    // `-v` audits scan coverage: list every item checked and its per-entry
    // result. The concise result summary is left to the stdout channel below.
    if verbose {
        print!("{}", render_checked(&scan.entries, &d));
    }

    // Notifiers always run: stdout prints the concise result (clean run
    // included), report updates its state file, and the alert channels
    // (desktop/email/webhook) self-suppress on a clean run.
    let notifiers = build_notifiers(&cfg.notify);
    let errors = dispatch(&notifiers, &report);
    for e in &errors {
        eprintln!("skillshield: notifier failure: {e}");
    }

    Ok(if report.has_changes() {
        Code::CHANGES
    } else {
        Code::OK
    })
}

/// Render the full list of checked items with per-entry status, for `-v`.
/// Unchanged entries show `ok`; changed ones show their `ChangeKind`. Removed
/// entries live only in the baseline (not in `entries`), so they are listed
/// from the diff.
fn render_checked(entries: &[Entry], d: &ScanDiff) -> String {
    use std::collections::BTreeMap;
    let status: BTreeMap<&std::path::PathBuf, ChangeKind> =
        d.findings.iter().map(|f| (&f.path, f.change)).collect();

    let mut s = format!("Checked {} item(s):\n", entries.len());
    for e in entries {
        let label = match status.get(&e.path) {
            Some(ChangeKind::Added) => "added",
            Some(ChangeKind::Modified) => "modified",
            _ => "ok",
        };
        s.push_str(&format!("  {label:<8}  {}\n", e.path.display()));
    }
    for f in d
        .findings
        .iter()
        .filter(|f| f.change == ChangeKind::Removed)
    {
        s.push_str(&format!("  {:<8}  {}\n", "removed", f.path.display()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillshield_core::diff::Finding;
    use skillshield_core::entry::EntryKind;

    fn entry(path: &str) -> Entry {
        Entry {
            path: path.into(),
            kind: EntryKind::File,
            digest: Some("sha256:x".into()),
            symlink_target: None,
            size: 1,
            mtime: 0,
            unhashed: false,
            source_rule: "r".into(),
        }
    }

    fn finding(path: &str, change: ChangeKind) -> Finding {
        Finding {
            path: path.into(),
            change,
            kind: EntryKind::File,
            rule_id: "r".into(),
            old_digest: None,
            new_digest: None,
            detail: String::new(),
        }
    }

    #[test]
    fn render_checked_labels_each_entry_and_lists_removed() {
        let entries = vec![entry("/a"), entry("/b")];
        let d = ScanDiff {
            findings: vec![
                finding("/b", ChangeKind::Modified),
                finding("/gone", ChangeKind::Removed),
            ],
        };
        let out = render_checked(&entries, &d);
        assert!(out.contains("Checked 2 item(s):"));

        // Find each item's line and check its status label, robustly to padding.
        let line = |needle: &str| {
            out.lines()
                .find(|l| l.ends_with(needle))
                .unwrap_or_else(|| panic!("no line for {needle} in:\n{out}"))
                .trim()
        };
        assert!(line("/a").starts_with("ok"));
        assert!(line("/b").starts_with("modified"));
        // Removed entry is not among `entries`; it comes from the diff.
        assert!(line("/gone").starts_with("removed"));
    }
}
