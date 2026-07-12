//! The catalog of what to watch: match rules (exact path / glob / directory
//! file-set) and the built-in defaults for known AI-agent artifacts.
//!
//! Global rules are organized into named **groups** (e.g. `claude.core`) so the
//! user can choose which agents/locations to monitor. The catalog deliberately
//! targets the files agents actually *load as behavior* (skills, plugins,
//! commands, agents, hooks, settings, instruction files, MCP config) rather
//! than whole home directories, whose bulk is churny runtime state
//! (sandboxes, sessions, caches, logs) that would drown a tripwire in noise.

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
    fn retain_groups_none_keeps_everything() {
        let before = Catalog::builtin().rules.len();
        let after = Catalog::builtin().retain_groups(None).rules.len();
        assert_eq!(before, after);
    }
}
