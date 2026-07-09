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
