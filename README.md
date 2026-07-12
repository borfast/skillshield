# SkillShield

A Linux tripwire for the files AI coding agents consume — skills, plugins,
`CLAUDE.md`/`AGENTS.md`, MCP configs, and more.

The goal is to have some insurance that if a malicious file is added to an
agent, or an existing file is modified in some way to make it malicious,
it does not pass silently.

It baselines what exists, then warns you when anything is added, modified,
or removed. Detect-and-warn only: it never edits or blocks your files.

## Install

```bash
cargo install --path crates/skillshield-cli
```

## Quick start

```bash
skillshield config               # show effective settings, paths, and what gets scanned
skillshield init                 # pick what to monitor, discover artifacts, write baseline
skillshield init --yes           # non-interactive: recommended groups, trust all
skillshield monitor ~/projects/x # add a project directory to watch
skillshield add-profile claude ~/.claude-gc  # watch an extra agent profile dir
skillshield scan                 # check for changes (exit 10 if any)
skillshield scan -v              # also list every item checked and its result
skillshield status               # human-readable diff
skillshield review               # accept/reject pending changes
skillshield schedule             # install a periodic scan (systemd timer or cron)
```

## What it monitors

SkillShield targets the files agents actually **load as behavior** — skills,
plugins, commands, agents, hooks, settings, instruction files (`CLAUDE.md`/
`AGENTS.md`/`GEMINI.md`), and MCP config — grouped per agent (`claude.core`,
`claude.config`, `claude.memory`, `codex.core`, `codex.config`, `gemini`,
`cursor`, `copilot`). It deliberately does **not** watch whole agent home
directories, whose bulk is churny runtime state (sandboxes, sessions, caches,
logs) that would drown a tripwire in noise.

At `init` you pick which groups to monitor (a checkbox picker; recommended
groups that exist are pre-selected). The choice is saved to
`[catalog].monitor` in the config and shown by `skillshield config`. Per-project
files are covered separately via `skillshield monitor <path>`.

### Extra agent profiles

If an agent's profile lives in a non-standard directory (e.g. a second
`CLAUDE_CONFIG_DIR` at `~/.claude-gc`), register it with:

```bash
skillshield add-profile claude ~/.claude-gc      # also: codex, gemini
skillshield add-profile claude ~/.claude-gc --remove
```

This re-roots that agent's rules at the given directory as its own selectable
groups (e.g. `claude.core@claude-gc`), recorded under `[[catalog.profiles]]`.

## Scheduling

`skillshield schedule` installs a periodic `scan`, auto-detecting a **systemd
user timer** (preferred) or falling back to **cron**. It prints exactly what it
will write/run and asks before touching anything; re-running is idempotent.

```bash
skillshield schedule                 # hourly, auto-detected backend, with a prompt
skillshield schedule --interval daily --time 09:00
skillshield schedule --cron --yes    # force cron, skip the prompt
skillshield schedule --remove        # tear it down
```

On a clean run `scan` still prints a one-line result to stdout (a useful
heartbeat in journald/cron logs), but the **alert** channels
(desktop/email/webhook) stay quiet — they only fire when something changed. Use
`skillshield scan -v` to also list every item checked. Hand-managed
systemd/cron examples remain in `packaging/`.

Config: `~/.config/skillshield/config.toml`.
State: `~/.local/share/skillshield/{baseline.json,last-report.json}`.

### Notification channels

Enable channels in `[notify].channels`; each has its own table. Email supports
`sendmail` (default) or `smtp`:

```toml
[notify]
channels = ["report", "stdout", "email"]

[notify.email]
to = "me@example.com"
from = "skillshield@myhost"
transport = "smtp"           # or "sendmail"

[notify.email.smtp]
host = "smtp.example.com"
port = 587
username = "me@example.com"
password = "app-password"
starttls = true

# Generic webhook (ntfy/Slack/Telegram/Discord):
[notify.webhook]
url = "https://ntfy.sh/my-topic"
headers = [["Title", "SkillShield alert"]]
```

See `packaging/` for Systemd/cron scheduling.
