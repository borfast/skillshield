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
