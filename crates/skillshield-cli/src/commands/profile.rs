//! `skillshield add-profile <agent> <path>` — register (or remove) an agent
//! profile whose directory is in a non-standard location (e.g. a second
//! `CLAUDE_CONFIG_DIR`). Mirrors `monitor`: records it in config and folds its
//! files into the baseline.

use crate::commands::{abs, discover_now, to_err, write_config};
use crate::exit::Code;
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::{group_default_on, profileable_agents, Catalog};
use skillshield_core::config::{Config, Profile};
use skillshield_core::paths;
use std::path::Path;

pub fn run(agent: &str, path: &str) -> Result<i32, String> {
    if !profileable_agents().contains(&agent) {
        return Err(format!(
            "unknown agent '{agent}'. Supported: {}.",
            profileable_agents().join(", ")
        ));
    }
    // Normalize to an absolute path (like `monitor`) so it matches regardless
    // of the CWD a later `scan` runs from, and so `~/.claude-gc` and
    // `~/.claude-gc/` are the same profile.
    let dir = abs(Path::new(path));
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let profile = Profile {
        agent: agent.to_string(),
        path: dir.to_string_lossy().to_string(),
    };

    let mut cfg = Config::load().map_err(to_err)?;
    if cfg.catalog.profiles.contains(&profile) {
        println!("Profile already registered; refreshing baseline.");
    } else {
        cfg.catalog.profiles.push(profile.clone());
        // Monitor the profile's default-on groups (core/config), matching the
        // primary install's defaults — `claude.memory@…` stays opt-in. If the
        // monitor allowlist is `None` (all), it already includes them.
        let default_groups: Vec<String> = profile_group_keys(&profile)
            .into_iter()
            .filter(|g| group_default_on(g))
            .collect();
        if let Some(monitor) = cfg.catalog.monitor.as_mut() {
            for g in &default_groups {
                if !monitor.contains(g) {
                    monitor.push(g.clone());
                }
            }
        }
        write_config(&cfg)?;
        println!(
            "Added {agent} profile {} (monitoring: {}).",
            dir.display(),
            default_groups.join(", ")
        );
    }

    // Trust everything under the profile root.
    let (scan, _cfg) = discover_now()?;
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    let mut baseline = if baseline_path.exists() {
        Baseline::load(&baseline_path).map_err(to_err)?
    } else {
        Baseline::new(vec![])
    };
    let mut added = 0;
    for e in scan.entries.iter().filter(|e| e.path.starts_with(&dir)) {
        baseline.upsert(e.clone());
        added += 1;
    }
    baseline.save(&baseline_path).map_err(to_err)?;
    println!("Trusted {added} file(s) under {}.", dir.display());
    Ok(Code::OK)
}

pub fn run_remove(agent: &str, path: &str) -> Result<i32, String> {
    let dir = abs(Path::new(path));
    let profile = Profile {
        agent: agent.to_string(),
        path: dir.to_string_lossy().to_string(),
    };
    let mut cfg = Config::load().map_err(to_err)?;
    let before = cfg.catalog.profiles.len();
    cfg.catalog.profiles.retain(|p| p != &profile);
    if cfg.catalog.profiles.len() == before {
        return Err(format!("no such profile: {agent} {}", dir.display()));
    }
    // Drop all of this profile's groups from the monitor allowlist if present.
    let groups = profile_group_keys(&profile);
    if let Some(monitor) = cfg.catalog.monitor.as_mut() {
        monitor.retain(|g| !groups.contains(g));
    }
    write_config(&cfg)?;

    let baseline_path = paths::baseline_path().map_err(to_err)?;
    if baseline_path.exists() {
        let mut baseline = Baseline::load(&baseline_path).map_err(to_err)?;
        let removed = baseline.remove_under(&dir);
        baseline.save(&baseline_path).map_err(to_err)?;
        println!(
            "Removed profile {agent} {}; pruned {removed} baseline entry/entries.",
            dir.display()
        );
    } else {
        println!("Removed profile {agent} {} from config.", dir.display());
    }
    Ok(Code::OK)
}

/// The namespaced group keys a profile contributes. Since `with_profiles` on a
/// single profile only adds that profile's `@`-suffixed groups, any group
/// containing `@` belongs to it.
fn profile_group_keys(profile: &Profile) -> Vec<String> {
    let mut groups: Vec<String> = Catalog::builtin()
        .with_profiles(std::slice::from_ref(profile))
        .rules
        .iter()
        .filter(|r| r.group.contains('@'))
        .map(|r| r.group.clone())
        .collect();
    groups.sort();
    groups.dedup();
    groups
}
