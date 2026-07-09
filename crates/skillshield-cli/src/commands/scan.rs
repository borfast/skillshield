use crate::commands::{discover_now, load_baseline_or_hint};
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

    Ok(if report.has_changes() { Code::CHANGES } else { Code::OK })
}
