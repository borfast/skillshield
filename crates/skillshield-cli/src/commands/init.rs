use crate::commands::{to_err, write_config};
use crate::review_ui::group_entries;
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::Catalog;
use skillshield_core::config::Config;
use skillshield_core::discovery::discover;
use skillshield_core::notify::desktop::graphical_session_available;
use skillshield_core::paths;
use std::io::{self, Write};

pub fn run(force: bool) -> Result<i32, String> {
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    if baseline_path.exists() && !force {
        return Err(format!(
            "baseline already exists at {}. Use `skillshield scan`/`review`, or `init --force` to rebuild.",
            baseline_path.display()
        ));
    }

    let cfg = Config::load().map_err(to_err)?;
    let catalog = Catalog::builtin().apply(&cfg.catalog.disable, &cfg.catalog.extra_files);
    let scan = discover(&catalog, &cfg.scan);

    if scan.entries.is_empty() {
        println!("No agent artifacts found. Nothing to baseline yet.");
    }
    let groups = group_entries(&scan.entries);
    println!("Found {} file(s) across {} group(s):", scan.entries.len(), groups.len());
    for g in &groups {
        println!("  [{}] {} file(s)", g.key, g.entries_idx.len());
    }
    for e in &scan.errors {
        eprintln!("  ! could not read {} — {}", e.path.display(), e.message);
    }

    print!("Trust all discovered files as the baseline? [y/N] ");
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(to_err)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        println!("Aborted. No baseline written.");
        return Ok(0);
    }

    let baseline = Baseline::new(scan.entries.clone());
    baseline.save(&baseline_path).map_err(to_err)?;
    println!("Baseline written to {}", baseline_path.display());

    maybe_setup_desktop(&cfg)?;
    print_scheduling_hint();
    Ok(0)
}

fn maybe_setup_desktop(cfg: &Config) -> Result<(), String> {
    if !graphical_session_available() {
        println!(
            "No graphical session detected; 'desktop' notifications left disabled. \
             Add \"desktop\" to notify.channels in config.toml to enable."
        );
        return Ok(());
    }
    // Persist "desktop" into the config channels if missing.
    let config_path = paths::config_path().map_err(to_err)?;
    let mut cfg = cfg.clone();
    if !cfg.notify.channels.iter().any(|c| c == "desktop") {
        cfg.notify.channels.push("desktop".into());
        write_config(&cfg)?;
        println!("Enabled 'desktop' notifications in {}", config_path.display());
    }
    // Send a one-off test notification.
    let _ = notify_rust::Notification::new()
        .summary("SkillShield")
        .body("Desktop notifications are working.")
        .show();
    println!("Sent a test desktop notification (check your notifications).");
    Ok(())
}

fn print_scheduling_hint() {
    println!(
        "\nTo run periodically:\n  systemctl --user enable --now skillshield.timer\n  \
         (or add a cron entry — see packaging/ in the repo)"
    );
}
