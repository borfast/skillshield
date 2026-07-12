use crate::commands::{to_err, write_config};
use crate::review_ui::group_entries;
use crate::tui::{self, GroupChoice};
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::{global_groups, group_default_on, profile_name, Catalog, Scope};
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

/// Build the picker rows: one per global group, with a file count (or "not
/// found") and pre-checked per the recommended default, or per an existing
/// `monitor` selection when re-running `init`.
fn build_group_choices(cfg: &Config) -> Vec<GroupChoice> {
    // Enumerate every global group in the profiled catalog (built-in + profile
    // groups). Apply per-rule `disable` so the picker's counts match what would
    // actually be scanned.
    let catalog = Catalog::builtin()
        .with_profiles(&cfg.catalog.profiles)
        .apply(&cfg.catalog.disable, &[]);
    let existing = cfg.catalog.monitor.clone();

    let mut groups: Vec<String> = Vec::new();
    for r in catalog.rules.iter().filter(|r| r.scope == Scope::Global) {
        if !groups.contains(&r.group) {
            groups.push(r.group.clone());
        }
    }

    groups
        .into_iter()
        .map(|group| {
            let rules: Vec<_> = catalog
                .rules
                .iter()
                .filter(|r| r.scope == Scope::Global && r.group == group)
                .cloned()
                .collect();
            let count = discover(&Catalog { rules }, &ScanConfig::default())
                .entries
                .len();
            let exists = count > 0;
            let detail = if exists {
                format!("{count} file(s)")
            } else {
                "not found".into()
            };
            let (label, default_on) = group_display(&group);
            let checked = match &existing {
                Some(sel) => sel.iter().any(|g| g == &group),
                None => default_on && exists,
            };
            GroupChoice {
                key: group,
                label,
                detail,
                checked,
            }
        })
        .collect()
}

/// Human label + default-on flag for a group key. Profile groups (`base@name`)
/// derive their label from the base group's description plus the profile name.
fn group_display(group: &str) -> (String, bool) {
    if let Some(meta) = global_groups().into_iter().find(|m| m.key == group) {
        return (meta.description.to_string(), meta.default_on);
    }
    if let Some((base, path)) = group.split_once('@') {
        let base_desc = global_groups()
            .into_iter()
            .find(|m| m.key == base)
            .map(|m| m.description.to_string())
            .unwrap_or_else(|| base.to_string());
        // Friendly label (basename), but the group KEY keeps the full path;
        // inherit the base group's default-on (so memory@… stays opt-in).
        return (
            format!("{base_desc}  @ {}", profile_name(path)),
            group_default_on(group),
        );
    }
    (group.to_string(), true)
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
