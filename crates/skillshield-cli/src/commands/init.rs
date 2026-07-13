use crate::commands::{to_err, write_config};
use crate::review_ui::group_entries;
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::{group_default_on, Catalog, Scope};
use skillshield_core::config::{Config, ScanConfig};
use skillshield_core::discovery::discover;
use skillshield_core::notify::desktop::graphical_session_available;
use skillshield_core::paths;
use std::io::{self, Write};

pub fn run(force: bool, yes: bool) -> Result<i32, String> {
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    if baseline_path.exists() && !force {
        return Err(format!(
            "baseline already exists at {}. Use `skillshield scan`/`review`, or `init --force` to rebuild.",
            baseline_path.display()
        ));
    }

    let mut cfg = Config::load().map_err(to_err)?;

    // Materialize a default config file if none exists, so the user has a
    // concrete file to inspect and customize.
    if let Some(path) = crate::commands::ensure_config_file()? {
        println!(
            "Created a default config file at {} — edit it to customize.\n",
            path.display()
        );
    }

    // Decide what to monitor. Keep an explicit selection if the config already
    // has one (hand-edited or from a prior run); otherwise select the
    // recommended groups (default-on and present on this machine). Customize by
    // editing `[catalog].monitor` — no interactive prompt.
    if cfg.catalog.monitor.is_none() {
        cfg.catalog.monitor = Some(recommended_monitor(&cfg));
        write_config(&cfg)?;
    }

    // Orient the user (this reflects the monitor selection).
    print!("{}", crate::commands::config::overview_for(&cfg)?);
    println!(
        "\nTo change what's monitored, edit [catalog].monitor in the config \
         (see `skillshield config`) and re-run `skillshield init --force`.\n"
    );

    println!("Scanning monitored locations…\n");
    let catalog = Catalog::builtin()
        .with_profiles(&cfg.catalog.profiles)
        .apply(&cfg.catalog.disable, &cfg.catalog.extra_files)
        .retain_groups(cfg.catalog.monitor.as_deref());
    let scan = discover(&catalog, &cfg.scan);

    if scan.entries.is_empty() {
        println!("No agent artifacts found. Nothing to baseline yet.");
    }
    let groups = group_entries(&scan.entries);
    println!(
        "Found {} file(s) across {} group(s):",
        scan.entries.len(),
        groups.len()
    );
    for grp in &groups {
        println!("  [{}] {} file(s)", grp.key, grp.entries_idx.len());
    }
    for e in &scan.errors {
        eprintln!("  ! could not read {} — {}", e.path.display(), e.message);
    }

    let trust = if yes {
        true
    } else {
        print!("Trust all discovered files as the baseline? [y/N] ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(to_err)?;
        matches!(answer.trim(), "y" | "Y" | "yes")
    };
    if !trust {
        println!("Aborted. No baseline written.");
        return Ok(0);
    }

    let baseline = Baseline::new(scan.entries);
    baseline.save(&baseline_path).map_err(to_err)?;
    println!("Baseline written to {}", baseline_path.display());

    maybe_setup_desktop(&cfg)?;
    print_scheduling_hint();
    Ok(0)
}

/// The recommended `monitor` allowlist for a fresh setup: every global group
/// that is default-on and actually present on this machine.
fn recommended_monitor(cfg: &Config) -> Vec<String> {
    let catalog = Catalog::builtin()
        .with_profiles(&cfg.catalog.profiles)
        .apply(&cfg.catalog.disable, &[]);

    let mut seen: Vec<String> = Vec::new();
    let mut selected: Vec<String> = Vec::new();
    for r in catalog.rules.iter().filter(|r| r.scope == Scope::Global) {
        if seen.contains(&r.group) {
            continue;
        }
        seen.push(r.group.clone());
        if !group_default_on(&r.group) {
            continue;
        }
        let rules: Vec<_> = catalog
            .rules
            .iter()
            .filter(|x| x.scope == Scope::Global && x.group == r.group)
            .cloned()
            .collect();
        let present = !discover(&Catalog { rules }, &ScanConfig::default())
            .entries
            .is_empty();
        if present {
            selected.push(r.group.clone());
        }
    }
    selected
}

fn maybe_setup_desktop(cfg: &Config) -> Result<(), String> {
    if !graphical_session_available() {
        println!(
            "No graphical session detected; 'desktop' notifications left disabled. \
             Add \"desktop\" to notify.channels in config.toml to enable."
        );
        return Ok(());
    }
    let config_path = paths::config_path().map_err(to_err)?;
    let mut cfg = cfg.clone();
    if !cfg.notify.channels.iter().any(|c| c == "desktop") {
        cfg.notify.channels.push("desktop".into());
        write_config(&cfg)?;
        println!(
            "Enabled 'desktop' notifications in {}",
            config_path.display()
        );
    }
    let _ = notify_rust::Notification::new()
        .summary("SkillShield")
        .body("Desktop notifications are working.")
        .show();
    println!("Sent a test desktop notification (check your notifications).");
    Ok(())
}

fn print_scheduling_hint() {
    println!(
        "\nTo run periodically:\n  skillshield schedule        # installs a systemd timer or cron job (asks first)\n  \
         skillshield schedule --help # options: --interval, --time, --cron, --remove"
    );
}
