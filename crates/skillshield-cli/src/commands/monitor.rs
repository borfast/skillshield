use crate::commands::{abs, discover_now, to_err, write_config};
use crate::exit::Code;
use skillshield_core::baseline::Baseline;
use skillshield_core::config::Config;
use skillshield_core::paths;
use std::path::Path;

pub fn run(path: &Path) -> Result<i32, String> {
    let target = abs(path);
    if !target.is_dir() {
        return Err(format!("{} is not a directory", target.display()));
    }
    let mut cfg = Config::load().map_err(to_err)?;
    let target_str = target.to_string_lossy().to_string();
    if !cfg.scan.project_roots.iter().any(|r| r == &target_str) {
        cfg.scan.project_roots.push(target_str.clone());
        write_config(&cfg)?;
        println!("Added project root {} to config.", target.display());
    } else {
        println!(
            "{} already monitored; refreshing baseline.",
            target.display()
        );
    }

    // Discover with the updated config, trust everything under this root.
    let (scan, _cfg) = discover_now()?;
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    let mut baseline = if baseline_path.exists() {
        Baseline::load(&baseline_path).map_err(to_err)?
    } else {
        Baseline::new(vec![])
    };
    let mut added = 0;
    for e in scan.entries.iter().filter(|e| e.path.starts_with(&target)) {
        baseline.upsert(e.clone());
        added += 1;
    }
    baseline.save(&baseline_path).map_err(to_err)?;
    println!("Trusted {added} file(s) under {}.", target.display());
    Ok(Code::OK)
}

pub fn run_unmonitor(path: &Path) -> Result<i32, String> {
    let target = abs(path);
    let mut cfg = Config::load().map_err(to_err)?;
    let target_str = target.to_string_lossy().to_string();
    let before = cfg.scan.project_roots.len();
    cfg.scan.project_roots.retain(|r| r != &target_str);
    if cfg.scan.project_roots.len() == before {
        return Err(format!(
            "{} is not a monitored project root.",
            target.display()
        ));
    }
    write_config(&cfg)?;

    let baseline_path = paths::baseline_path().map_err(to_err)?;
    if baseline_path.exists() {
        let mut baseline = Baseline::load(&baseline_path).map_err(to_err)?;
        let removed = baseline.remove_under(&target);
        baseline.save(&baseline_path).map_err(to_err)?;
        println!(
            "Removed {} and pruned {removed} baseline entry/entries.",
            target.display()
        );
    } else {
        println!(
            "Removed {} from config (no baseline to prune).",
            target.display()
        );
    }
    Ok(Code::OK)
}
