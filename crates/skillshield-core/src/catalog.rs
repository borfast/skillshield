//! The catalog of what to watch: match rules (exact path / glob / directory
//! file-set) and the built-in defaults for known AI-agent artifacts.

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
    pub description: String,
    pub spec: MatchSpec,
    pub scope: Scope,
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
                description: format!("user extra: {glob}"),
                spec: MatchSpec::Glob(glob.clone()),
                scope: Scope::Project,
            });
        }
        self
    }
}

fn global(id: &str, desc: &str, spec: MatchSpec) -> Rule {
    Rule {
        id: id.into(),
        description: desc.into(),
        spec,
        scope: Scope::Global,
    }
}

fn project(id: &str, desc: &str, spec: MatchSpec) -> Rule {
    Rule {
        id: id.into(),
        description: desc.into(),
        spec,
        scope: Scope::Project,
    }
}

pub fn default_rules() -> Vec<Rule> {
    use MatchSpec::*;
    vec![
        // ---- Global locations ----
        global(
            "claude.home",
            "Claude home top-level files",
            DirFileSet("~/.claude/".into()),
        ),
        global(
            "claude.skills",
            "Claude skills",
            DirFileSet("~/.claude/skills/".into()),
        ),
        global(
            "claude.plugins",
            "Claude plugins & marketplaces",
            DirFileSet("~/.claude/plugins/".into()),
        ),
        global(
            "claude.commands",
            "Claude commands",
            DirFileSet("~/.claude/commands/".into()),
        ),
        global(
            "claude.agents",
            "Claude agents",
            DirFileSet("~/.claude/agents/".into()),
        ),
        global(
            "claude.md.home",
            "Global CLAUDE.md",
            ExactPath("~/.claude/CLAUDE.md".into()),
        ),
        global(
            "claude.settings",
            "Claude settings",
            Glob("~/.claude/settings*.json".into()),
        ),
        global(
            "claude.mcp",
            "Claude MCP/project registry",
            ExactPath("~/.claude.json".into()),
        ),
        global(
            "claude.config.xdg",
            "Claude XDG config",
            DirFileSet("~/.config/claude/".into()),
        ),
        global("codex.home", "Codex home", DirFileSet("~/.codex/".into())),
        global(
            "codex.config.xdg",
            "Codex XDG config",
            DirFileSet("~/.config/codex/".into()),
        ),
        global(
            "gemini.home",
            "Gemini home",
            DirFileSet("~/.gemini/".into()),
        ),
        global(
            "gemini.md.home",
            "Global GEMINI.md",
            ExactPath("~/.gemini/GEMINI.md".into()),
        ),
        global(
            "gemini.config.xdg",
            "Gemini XDG config",
            DirFileSet("~/.config/gemini/".into()),
        ),
        global(
            "cursor.home",
            "Cursor home (rules, MCP)",
            DirFileSet("~/.cursor/".into()),
        ),
        global(
            "copilot.config.xdg",
            "GitHub Copilot config",
            DirFileSet("~/.config/github-copilot/".into()),
        ),
        global(
            "mcp.config.xdg",
            "MCP XDG config",
            DirFileSet("~/.config/mcp/".into()),
        ),
        // ---- Project artifact patterns ----
        project(
            "proj.claude.md",
            "Project CLAUDE.md",
            Glob("**/CLAUDE.md".into()),
        ),
        project(
            "proj.claude.local",
            "Project CLAUDE.local.md",
            Glob("**/CLAUDE.local.md".into()),
        ),
        project(
            "proj.agents.md",
            "Project AGENTS.md",
            Glob("**/AGENTS.md".into()),
        ),
        project(
            "proj.gemini.md",
            "Project GEMINI.md",
            Glob("**/GEMINI.md".into()),
        ),
        project(
            "proj.claude.dir",
            "Project .claude directory",
            DirFileSet("**/.claude/".into()),
        ),
        project(
            "proj.cursor.dir",
            "Project .cursor directory",
            DirFileSet("**/.cursor/".into()),
        ),
        project(
            "proj.cursorrules",
            "Project .cursorrules",
            Glob("**/.cursorrules".into()),
        ),
        project(
            "proj.mcp.json",
            "Project .mcp.json",
            Glob("**/.mcp.json".into()),
        ),
        project(
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
}
