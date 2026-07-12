//! `skillshield config` — print the effective configuration (annotated TOML),
//! where config/state live, and a summary of the built-in catalog, so the user
//! can see exactly what a scan will do. `init` reuses this as its preamble.

use crate::commands::to_err;
use crate::exit::Code;
use skillshield_core::catalog::{global_groups, Catalog, Scope};
use skillshield_core::config::Config;
use skillshield_core::paths;

pub fn run() -> Result<i32, String> {
    if let Some(path) = crate::commands::ensure_config_file()? {
        println!("Created a default config file at {}\n", path.display());
    }
    print!("{}", overview_for(&Config::load().map_err(to_err)?)?);
    Ok(Code::OK)
}

/// Build the overview text for an already-loaded config (used by `init` too).
pub fn overview_for(cfg: &Config) -> Result<String, String> {
    let config_path = paths::config_path().map_err(to_err)?;
    let baseline_path = paths::baseline_path().map_err(to_err)?;
    Ok(render_overview(
        cfg,
        &config_path.to_string_lossy(),
        config_path.exists(),
        &baseline_path.to_string_lossy(),
        baseline_path.exists(),
    ))
}

fn toml_array(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// Render the overview. Pure (given the config + resolved paths), so it is
/// unit-testable without touching the filesystem.
pub fn render_overview(
    cfg: &Config,
    config_path: &str,
    config_exists: bool,
    baseline_path: &str,
    baseline_exists: bool,
) -> String {
    let mut s = String::new();
    s.push_str("SkillShield — configuration overview\n\n");
    s.push_str(
        "SkillShield watches the files AI coding agents load (skills, plugins,\n\
         CLAUDE.md/AGENTS.md, MCP configs) and warns you when they change. It only\n\
         observes — it never edits or blocks your files.\n\n",
    );

    let cfg_note = if config_exists {
        "loaded from file"
    } else {
        "not present — using built-in defaults"
    };
    let base_note = if baseline_exists {
        "exists"
    } else {
        "not created yet"
    };
    s.push_str(&format!("Config:   {config_path}  ({cfg_note})\n"));
    s.push_str(&format!("Baseline: {baseline_path}  ({base_note})\n\n"));

    s.push_str("Effective configuration:\n\n");
    let sc = &cfg.scan;
    s.push_str("  [scan]\n");
    s.push_str(&format!(
        "  follow_symlinks   = {}    # never descend into symlinked directories\n",
        sc.follow_symlinks
    ));
    s.push_str(&format!(
        "  max_hash_bytes    = {}    # larger files are tracked but not hashed\n",
        sc.max_hash_bytes
    ));
    s.push_str(&format!(
        "  project_roots     = {}    # extra dirs you opted in (add with `skillshield monitor <path>`)\n",
        toml_array(&sc.project_roots)
    ));
    s.push_str(&format!(
        "  project_max_depth = {}    # how deep to crawl each project root\n",
        sc.project_max_depth
    ));
    s.push_str(&format!(
        "  ignore            = {}\n\n",
        toml_array(&sc.ignore)
    ));

    s.push_str("  [catalog]    # extend or trim the built-in catalog\n");
    s.push_str(&format!(
        "  extra_files = {}    # extra path globs to also watch\n",
        toml_array(&cfg.catalog.extra_files)
    ));
    s.push_str(&format!(
        "  disable     = {}    # built-in rule ids to turn off\n\n",
        toml_array(&cfg.catalog.disable)
    ));

    s.push_str("  [notify]\n");
    s.push_str(&format!(
        "  channels = {}    # where change alerts go (init adds \"desktop\" if a GUI is detected)\n",
        toml_array(&cfg.notify.channels)
    ));
    if cfg.notify.email.is_some() {
        s.push_str("  [notify.email]    (configured)\n");
    }
    if cfg.notify.webhook.is_some() {
        s.push_str("  [notify.webhook]    (configured)\n");
    }
    s.push('\n');

    let monitoring = match &cfg.catalog.monitor {
        Some(groups) if groups.is_empty() => "(nothing selected)".to_string(),
        Some(groups) => groups.join(", "),
        None => "all groups (run `skillshield init` to choose)".to_string(),
    };
    s.push_str(&format!("Monitoring groups: {monitoring}\n"));
    if !cfg.catalog.profiles.is_empty() {
        s.push_str("Profiles:\n");
        for p in &cfg.catalog.profiles {
            s.push_str(&format!("  {} @ {}\n", p.agent, p.path));
        }
    }
    s.push('\n');

    let catalog = Catalog::builtin();
    let global = catalog
        .rules
        .iter()
        .filter(|r| r.scope == Scope::Global)
        .count();
    let project = catalog
        .rules
        .iter()
        .filter(|r| r.scope == Scope::Project)
        .count();
    let group_count = global_groups().len();
    s.push_str(&format!(
        "The built-in catalog defines {global} global rules across {group_count} agent groups\n\
         (behavior files only: skills, plugins, commands, agents, hooks, settings,\n\
         instruction files, MCP config) plus {project} project-file patterns (CLAUDE.md,\n\
         AGENTS.md, .mcp.json, …) matched under any monitored project root. Only the\n\
         groups listed above are scanned.\n",
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_reflects_config_and_paths() {
        let mut cfg = Config::default();
        cfg.scan.max_hash_bytes = 123;
        cfg.notify.channels = vec!["report".into(), "webhook".into()];
        let out = render_overview(
            &cfg,
            "/cfg/config.toml",
            false,
            "/data/baseline.json",
            false,
        );

        // Sections + a reflected value + the paths + framing.
        assert!(out.contains("[scan]"));
        assert!(out.contains("max_hash_bytes    = 123"));
        assert!(out.contains("[notify]"));
        assert!(out.contains("\"webhook\""));
        assert!(out.contains("/cfg/config.toml"));
        assert!(out.contains("/data/baseline.json"));
        assert!(out.contains("using built-in defaults"));
        assert!(out.contains("never edits"));
        assert!(out.contains("built-in catalog"));
    }

    #[test]
    fn overview_marks_existing_config_as_loaded() {
        let out = render_overview(
            &Config::default(),
            "/cfg/config.toml",
            true,
            "/data/baseline.json",
            true,
        );
        assert!(out.contains("loaded from file"));
        assert!(out.contains("exists"));
    }
}
