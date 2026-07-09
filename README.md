# SkillShield

A Linux tripwire for the files AI coding agents consume — skills, plugins,
`CLAUDE.md`/`AGENTS.md`, MCP configs, and more.

The goal is to have some insurance that if a malicious file is added to an
agent, or an existing file is modified in some way to make it malicious,
it does pass silently.

It baselines what exists, then warns you when anything is added, modified,
or removed. Detect-and-warn only: it never edits or blocks your files.

## Install

```bash
cargo install --path crates/skillshield-cli
```

## Quick start

```bash
skillshield init                 # discover artifacts, review, write baseline
skillshield monitor ~/projects/x # add a project directory to watch
skillshield scan                 # check for changes (exit 10 if any)
skillshield status               # human-readable diff
skillshield review               # accept/reject pending changes
```

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
