use crate::commands::{to_err, write_config};
use crate::review_ui::group_entries;
use crate::tui::{self, GroupChoice};
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::{global_groups, Catalog, Scope};
use skillshield_core::config::{Config, ScanConfig};
use skillshield_core::discovery::discover;
use skillshield_core::notify::desktop::graphical_session_available;
use skillshield_core::paths;
use std::io::{self, IsTerminal, Write};

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

    // Orient the user: what init does, where config/state live, the effective
    // settings, and the built-in catalog.
    print!("{}", crate::commands::config::overview_for(&cfg)?);

    // Choose which catalog groups to monitor. On a TTY, show the picker; on a
    // non-interactive stream (cron/CI/piped) or with --yes, take the
    // recommended defaults.
    let choices = build_group_choices(&cfg);
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let selected = if interactive && !yes {
        match tui::select_groups(
            "Select which locations SkillShield should monitor (space to toggle):",
            choices,
        )? {
            Some(sel) => sel,
            None => {
                println!("\nAborted. No baseline written.");
                return Ok(0);
            }
        }
    } else {
        tui::selected_keys(&choices)
    };
    cfg.catalog.monitor = Some(selected.clone());
    write_config(&cfg)?;
    println!(
        "\nMonitoring: {}",
        if selected.is_empty() {
            "(nothing selected)".to_string()
        } else {
            selected.join(", ")
        }
    );

    println!("\nScanning monitored locations…\n");
    let catalog = Catalog::builtin()
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

/// Build the picker rows: one per global group, with a file count (or "not
/// found") and pre-checked per the recommended default, or per an existing
/// `monitor` selection when re-running `init`.
fn build_group_choices(cfg: &Config) -> Vec<GroupChoice> {
    // Apply per-rule `disable` so the picker's counts match what would actually
    // be scanned (extra_files add project rules, irrelevant to global groups).
    let builtin = Catalog::builtin().apply(&cfg.catalog.disable, &[]);
    let existing = cfg.catalog.monitor.clone();
    global_groups()
        .into_iter()
        .map(|meta| {
            let rules: Vec<_> = builtin
                .rules
                .iter()
                .filter(|r| r.scope == Scope::Global && r.group == meta.key)
                .cloned()
                .collect();
            let scan = discover(&Catalog { rules }, &ScanConfig::default());
            let count = scan.entries.len();
            let exists = count > 0;
            let detail = if exists {
                format!("{count} file(s)")
            } else {
                "not found".into()
            };
            let checked = match &existing {
                Some(sel) => sel.iter().any(|g| g == meta.key),
                None => meta.default_on && exists,
            };
            GroupChoice {
                key: meta.key.into(),
                label: meta.description.into(),
                detail,
                checked,
            }
        })
        .collect()
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
