# Packaging & scheduling

## Systemd (user-level)

Copy the units into your user unit directory and enable the timer:

```bash
mkdir -p ~/.config/systemd/user
cp skillshield.service skillshield.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now skillshield.timer
```

Check results with `journalctl --user -u skillshield.service` or read
`~/.local/share/skillshield/last-report.json`.

## Cron alternative

```cron
# Daily at 09:00; exit code 10 (changes) is fine, cron only cares about run.
0 9 * * * /home/youruser/.cargo/bin/skillshield scan >> ~/.local/share/skillshield/cron.log 2>&1
```

Neither is installed automatically — `skillshield init` prints these hints.
