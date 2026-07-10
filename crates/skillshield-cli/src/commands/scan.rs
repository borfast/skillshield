use crate::commands::{discover_now, load_baseline_or_hint};
use crate::exit::Code;
use skillshield_core::diff::diff;
use skillshield_core::notify::{build_notifiers, dispatch};
use skillshield_core::report::{now_secs, ScanReport};

pub fn run(verbose: bool) -> Result<i32, String> {
    let baseline = load_baseline_or_hint()?;
    let (scan, cfg) = discover_now()?;
    let d = diff(&baseline, &scan);
    let report = ScanReport::from_diff(&d, &scan.errors, now_secs());

    // A run is "notable" if something changed or discovery hit errors. By
    // default a quiet, nothing-to-report run notifies nobody — important for an
    // hourly background timer/cron job (no stdout line, no webhook POST, no log
    // append). `--verbose` opts into the "no changes" notifications too.
    let notable = report.has_changes() || !report.scan_errors.is_empty();
    if notable || verbose {
        let notifiers = build_notifiers(&cfg.notify);
        let errors = dispatch(&notifiers, &report);
        for e in &errors {
            eprintln!("skillshield: notifier failure: {e}");
        }
    }

    Ok(if report.has_changes() {
        Code::CHANGES
    } else {
        Code::OK
    })
}
