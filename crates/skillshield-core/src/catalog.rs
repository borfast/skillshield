//! The catalog of what to watch: match rules (exact path / glob / directory
//! file-set) and the built-in defaults for known AI-agent artifacts.
//!
//! Global rules are organized into named **groups** (e.g. `claude.core`) so the
//! user can choose which agents/locations to monitor. The catalog deliberately
//! targets the files agents actually *load as behavior* (skills, plugins,
//! commands, agents, hooks, settings, instruction files, MCP config) rather
//! than whole home directories, whose bulk is churny runtime state
//! (sandboxes, sessions, caches, logs) that would drown a tripwire in noise.

use crate::config::Profile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Global,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchSpec {
    ExactPath(String),
    Glob(String),
    DirFileSet(String),
}

impl MatchSpec {
    /// The path/pattern string this spec matches on.
    pub fn path(&self) -> &str {
        match self {
            MatchSpec::ExactPath(p) | MatchSpec::Glob(p) | MatchSpec::DirFileSet(p) => p,
        }
    }

    /// A copy of this spec with its path/pattern replaced (same variant).
    fn with_path(&self, new: String) -> MatchSpec {
        match self {
            MatchSpec::ExactPath(_) => MatchSpec::ExactPath(new),
            MatchSpec::Glob(_) => MatchSpec::Glob(new),
            MatchSpec::DirFileSet(_) => MatchSpec::DirFileSet(new),
        }
    }
}

/// The default home directory for an agent that supports profile re-rooting.
/// `None` for agents without a simple directory home (cursor/copilot).
pub fn agent_default_root(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("~/.claude"),
        "codex" => Some("~/.codex"),
        "gemini" => Some("~/.gemini"),
        _ => None,
    }
}

/// The agents that support profiles (have a directory home), for validation.
pub fn profileable_agents() -> &'static [&'static str] {
    &["claude", "codex", "gemini"]
}

/// Whether a group key is pre-selected by default. Profile groups
/// (`base@path`) inherit their base group's `default_on`, so a profile's
/// `claude.memory@…` stays opt-in just like the primary `claude.memory`.
pub fn group_default_on(key: &str) -> bool {
    let base = key.split_once('@').map_or(key, |(b, _)| b);
    global_groups()
        .iter()
        .find(|m| m.key == base)
        .map(|m| m.default_on)
        .unwrap_or(false)
}

/// A short, filesystem-derived name for a profile path (`~/.claude-gc` →
/// `claude-gc`), used to namespace its groups/ids.
pub fn profile_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().trim_start_matches('.').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "profile".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    /// Selection group this rule belongs to (e.g. `claude.core`). Project rules
    /// use `project`; user `extra_files` rules use `extra`.
    pub group: String,
    pub description: String,
    pub spec: MatchSpec,
    pub scope: Scope,
}

/// Metadata about a selectable global group, for the `init` picker and `config`.
pub struct GroupMeta {
    pub key: &'static str,
    pub description: &'static str,
    /// Whether it is pre-selected by default (when it exists on the machine).
    pub default_on: bool,
}

/// The selectable global groups, in display order.
pub fn global_groups() -> Vec<GroupMeta> {
    vec![
        GroupMeta {
            key: "claude.core",
            description: "Claude — skills, plugins, commands, agents",
            default_on: true,
        },
        GroupMeta {
            key: "claude.config",
            description: "Claude — settings, CLAUDE.md, MCP, hooks",
            default_on: true,
        },
        GroupMeta {
            key: "claude.memory",
            description: "Claude — MEMORY/ (injected memory)",
            default_on: false,
        },
        GroupMeta {
            key: "codex.core",
            description: "Codex — skills, plugins, prompts, rules",
            default_on: true,
        },
        GroupMeta {
            key: "codex.config",
            description: "Codex — config.toml, AGENTS.md",
            default_on: true,
        },
        GroupMeta {
            key: "gemini",
            description: "Gemini — settings.json, GEMINI.md",
            default_on: true,
        },
        GroupMeta {
            key: "cursor",
            description: "Cursor — mcp.json, rules",
            default_on: true,
        },
        GroupMeta {
            key: "copilot",
            description: "GitHub Copilot — config",
            default_on: true,
        },
    ]
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub rules: Vec<Rule>,
}

impl Catalog {
    pub fn builtin() -> Self {
        Catalog {
            rules: default_rules(),
        }
    }

    pub fn apply(mut self, disable: &[String], extra_files: &[String]) -> Self {
        self.rules.retain(|r| !disable.iter().any(|d| d == &r.id));
        for (i, glob) in extra_files.iter().enumerate() {
            self.rules.push(Rule {
                id: format!("extra.{i}"),
                group: "extra".into(),
                description: format!("user extra: {glob}"),
                spec: MatchSpec::Glob(glob.clone()),
                scope: Scope::Project,
            });
        }
        self
    }

    /// Append re-rooted copies of each profile's agent rules. For a profile
    /// `{agent, path}`, every built-in global rule whose path lives *under* the
    /// agent's default home (e.g. `~/.claude/…`) is copied with the prefix
    /// swapped to `path`; its id/group are namespaced by the full profile path
    /// (e.g. `claude.core@/home/u/.claude-gc`) so profiles sharing a basename
    /// don't collide. Rules outside the home (the MCP sibling `~/.claude.json`,
    /// cursor/copilot globs) are not re-rooted.
    pub fn with_profiles(mut self, profiles: &[Profile]) -> Self {
        let builtin: Vec<Rule> = self
            .rules
            .iter()
            .filter(|r| r.scope == Scope::Global)
            .cloned()
            .collect();
        for profile in profiles {
            let Some(root) = agent_default_root(&profile.agent) else {
                continue;
            };
            let prefix = format!("{root}/");
            // Namespace by the full path (not basename): two profiles sharing a
            // leaf name (e.g. ~/.claude-gc and ~/other/.claude-gc) must not
            // collide on group/id keys.
            let base = profile.path.trim_end_matches('/');
            for rule in &builtin {
                if let Some(rel) = rule.spec.path().strip_prefix(&prefix) {
                    self.rules.push(Rule {
                        id: format!("{}@{}", rule.id, base),
                        group: format!("{}@{}", rule.group, base),
                        description: format!("{} @ {}", rule.description, base),
                        spec: rule.spec.with_path(format!("{base}/{rel}")),
                        scope: Scope::Global,
                    });
                }
            }
        }
        self
    }

    /// Keep only the global rules whose group is in `monitor` (project and
    /// extra rules are always kept — they apply under opted-in project roots).
    /// `None` keeps everything (back-compat for hand-written configs).
    pub fn retain_groups(mut self, monitor: Option<&[String]>) -> Self {
        if let Some(groups) = monitor {
            self.rules
                .retain(|r| r.scope != Scope::Global || groups.iter().any(|g| g == &r.group));
        }
        self
    }
}

fn g(id: &str, group: &str, desc: &str, spec: MatchSpec) -> Rule {
    Rule {
        id: id.into(),
        group: group.into(),
        description: desc.into(),
        spec,
        scope: Scope::Global,
    }
}

fn p(id: &str, desc: &str, spec: MatchSpec) -> Rule {
    Rule {
        id: id.into(),
        group: "project".into(),
        description: desc.into(),
        spec,
        scope: Scope::Project,
    }
}

pub fn default_rules() -> Vec<Rule> {
    use MatchSpec::*;
    vec![
        // ---- Claude ----
        g(
            "claude.skills",
            "claude.core",
            "Claude skills",
            DirFileSet("~/.claude/skills/".into()),
        ),
        g(
            "claude.plugins",
            "claude.core",
            "Claude plugins & marketplaces",
            DirFileSet("~/.claude/plugins/".into()),
        ),
        g(
            "claude.commands",
            "claude.core",
            "Claude commands",
            DirFileSet("~/.claude/commands/".into()),
        ),
        g(
            "claude.agents",
            "claude.core",
            "Claude agents",
            DirFileSet("~/.claude/agents/".into()),
        ),
        g(
            "claude.md",
            "claude.config",
            "Global CLAUDE.md",
            ExactPath("~/.claude/CLAUDE.md".into()),
        ),
        g(
            "claude.settings",
            "claude.config",
            "Claude settings",
            Glob("~/.claude/settings*.json".into()),
        ),
        g(
            "claude.mcp",
            "claude.config",
            "Claude MCP/project registry",
            ExactPath("~/.claude.json".into()),
        ),
        g(
            "claude.hooks",
            "claude.config",
            "Claude hooks (run shell commands)",
            DirFileSet("~/.claude/hooks/".into()),
        ),
        g(
            "claude.memory",
            "claude.memory",
            "Claude injected memory",
            DirFileSet("~/.claude/MEMORY/".into()),
        ),
        // ---- Codex ----
        g(
            "codex.skills",
            "codex.core",
            "Codex skills",
            DirFileSet("~/.codex/skills/".into()),
        ),
        g(
            "codex.plugins",
            "codex.core",
            "Codex plugins",
            DirFileSet("~/.codex/plugins/".into()),
        ),
        g(
            "codex.prompts",
            "codex.core",
            "Codex prompts",
            DirFileSet("~/.codex/prompts/".into()),
        ),
        g(
            "codex.rules",
            "codex.core",
            "Codex rules",
            DirFileSet("~/.codex/rules/".into()),
        ),
        g(
            "codex.config",
            "codex.config",
            "Codex config.toml",
            ExactPath("~/.codex/config.toml".into()),
        ),
        g(
            "codex.md",
            "codex.config",
            "Global Codex AGENTS.md",
            ExactPath("~/.codex/AGENTS.md".into()),
        ),
        // ---- Gemini ----
        g(
            "gemini.settings",
            "gemini",
            "Gemini settings",
            ExactPath("~/.gemini/settings.json".into()),
        ),
        g(
            "gemini.md",
            "gemini",
            "Global GEMINI.md",
            ExactPath("~/.gemini/GEMINI.md".into()),
        ),
        // ---- Cursor ----
        g(
            "cursor.mcp",
            "cursor",
            "Cursor MCP config",
            ExactPath("~/.cursor/mcp.json".into()),
        ),
        g(
            "cursor.rules",
            "cursor",
            "Cursor rules",
            DirFileSet("~/.cursor/rules/".into()),
        ),
        // ---- GitHub Copilot (instructions + MCP scattered under per-client subdirs) ----
        g(
            "copilot.instructions",
            "copilot",
            "Copilot custom instructions",
            Glob("~/.config/github-copilot/**/*instructions*.md".into()),
        ),
        g(
            "copilot.mcp",
            "copilot",
            "Copilot MCP config",
            Glob("~/.config/github-copilot/**/mcp.json".into()),
        ),
        // ---- Project artifact patterns (matched only under opted-in roots) ----
        p(
            "proj.claude.md",
            "Project CLAUDE.md",
            Glob("**/CLAUDE.md".into()),
        ),
        p(
            "proj.claude.local",
            "Project CLAUDE.local.md",
            Glob("**/CLAUDE.local.md".into()),
        ),
        p(
            "proj.agents.md",
            "Project AGENTS.md",
            Glob("**/AGENTS.md".into()),
        ),
        p(
            "proj.gemini.md",
            "Project GEMINI.md",
            Glob("**/GEMINI.md".into()),
        ),
        p(
            "proj.claude.dir",
            "Project .claude directory",
            DirFileSet("**/.claude/".into()),
        ),
        p(
            "proj.cursor.dir",
            "Project .cursor directory",
            DirFileSet("**/.cursor/".into()),
        ),
        p(
            "proj.cursorrules",
            "Project .cursorrules",
            Glob("**/.cursorrules".into()),
        ),
        p(
            "proj.mcp.json",
            "Project .mcp.json",
            Glob("**/.mcp.json".into()),
        ),
        p(
            "proj.github.copilot",
            "Copilot instructions",
            Glob("**/.github/copilot-instructions.md".into()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_known_rules() {
        let c = Catalog::builtin();
        assert!(c.rules.iter().any(|r| r.id == "claude.skills"));
        assert!(c.rules.iter().any(|r| r.id == "proj.agents.md"));
    }

    #[test]
    fn ids_are_unique() {
        let c = Catalog::builtin();
        let mut ids: Vec<_> = c.rules.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate rule ids");
    }

    #[test]
    fn apply_disables_and_extends() {
        let c = Catalog::builtin().apply(
            &["claude.skills".to_string()],
            &["**/MYAGENT.md".to_string()],
        );
        assert!(!c.rules.iter().any(|r| r.id == "claude.skills"));
        assert!(c.rules.iter().any(|r| r.id == "extra.0"));
    }

    #[test]
    fn no_recursive_whole_home_rules() {
        // The overly-broad whole-home / whole-XDG rules must be gone.
        for r in Catalog::builtin().rules {
            if let MatchSpec::DirFileSet(p) = &r.spec {
                let broad = [
                    "~/.claude/",
                    "~/.codex/",
                    "~/.gemini/",
                    "~/.cursor/",
                    "~/.config/claude/",
                    "~/.config/codex/",
                    "~/.config/gemini/",
                    "~/.config/mcp/",
                ];
                assert!(
                    !broad.contains(&p.as_str()),
                    "recursive whole-home rule leaked: {}",
                    r.id
                );
            }
        }
    }

    #[test]
    fn every_global_rule_has_a_known_group() {
        let keys: Vec<&str> = global_groups().iter().map(|g| g.key).collect();
        for r in Catalog::builtin()
            .rules
            .iter()
            .filter(|r| r.scope == Scope::Global)
        {
            assert!(
                keys.contains(&r.group.as_str()),
                "rule {} has unknown group {}",
                r.id,
                r.group
            );
        }
    }

    #[test]
    fn retain_groups_filters_globals_but_keeps_project() {
        let c = Catalog::builtin().retain_groups(Some(&["claude.core".to_string()]));
        // kept: claude.core globals + all project rules
        assert!(c.rules.iter().any(|r| r.id == "claude.skills"));
        assert!(c.rules.iter().any(|r| r.id == "proj.agents.md"));
        // dropped: a global from a non-selected group
        assert!(!c.rules.iter().any(|r| r.id == "gemini.md"));
        assert!(!c.rules.iter().any(|r| r.id == "claude.memory"));
    }

    #[test]
    fn with_profiles_reroots_agent_rules() {
        use crate::config::Profile;
        let c = Catalog::builtin().with_profiles(&[Profile {
            agent: "claude".into(),
            path: "/home/u/.claude-gc".into(),
        }]);
        // A re-rooted skills rule exists under the profile path, namespaced by
        // the FULL path (not basename).
        let r = c
            .rules
            .iter()
            .find(|r| r.id == "claude.skills@/home/u/.claude-gc")
            .expect("re-rooted skills rule");
        assert_eq!(r.group, "claude.core@/home/u/.claude-gc");
        assert_eq!(r.spec.path(), "/home/u/.claude-gc/skills/");
        // The MCP sibling (~/.claude.json, not under ~/.claude/) is NOT re-rooted.
        assert!(!c.rules.iter().any(|r| r.id.starts_with("claude.mcp@")));
    }

    #[test]
    fn same_basename_profiles_do_not_collide() {
        use crate::config::Profile;
        let c = Catalog::builtin().with_profiles(&[
            Profile {
                agent: "claude".into(),
                path: "/home/a/.claude-gc".into(),
            },
            Profile {
                agent: "claude".into(),
                path: "/home/b/.claude-gc".into(),
            },
        ]);
        // Distinct full-path namespaces → distinct group keys, no collision.
        assert!(c
            .rules
            .iter()
            .any(|r| r.group == "claude.core@/home/a/.claude-gc"));
        assert!(c
            .rules
            .iter()
            .any(|r| r.group == "claude.core@/home/b/.claude-gc"));
    }

    #[test]
    fn group_default_on_inherits_base() {
        assert!(group_default_on("claude.core@/home/u/.claude-gc"));
        assert!(!group_default_on("claude.memory@/home/u/.claude-gc"));
        assert!(!group_default_on("claude.memory"));
    }

    #[test]
    fn with_profiles_ignores_unprofileable_agent() {
        use crate::config::Profile;
        let before = Catalog::builtin().rules.len();
        let c = Catalog::builtin().with_profiles(&[Profile {
            agent: "cursor".into(),
            path: "~/x".into(),
        }]);
        assert_eq!(c.rules.len(), before, "unprofileable agent must be a no-op");
    }

    #[test]
    fn retain_groups_none_keeps_everything() {
        let before = Catalog::builtin().rules.len();
        let after = Catalog::builtin().retain_groups(None).rules.len();
        assert_eq!(before, after);
    }
}
