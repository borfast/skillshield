use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "skillshield",
    version,
    about = "Tripwire for AI-agent config artifacts"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build the trusted baseline (first run).
    Init {
        /// Overwrite an existing baseline.
        #[arg(long)]
        force: bool,
        /// Non-interactive: take the recommended monitored groups and trust all
        /// discovered files without prompting (for scripted installs).
        #[arg(long)]
        yes: bool,
    },
    /// Scan and report changes vs. the baseline (scheduled use). Read-only.
    Scan {
        /// Also list every file/directory checked and its per-entry result
        /// (ok/added/modified/removed), in addition to the result summary.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show current changes vs. the baseline. Read-only.
    Status,
    /// Show the effective configuration, paths, and built-in catalog summary.
    Config,
    /// Interactively accept/reject pending changes.
    Review,
    /// Accept a specific path into the baseline.
    Trust { path: PathBuf },
    /// Add a project root: crawl once, record in config, trust findings.
    Monitor { path: PathBuf },
    /// Forget a monitored project root: remove it from config and prune its
    /// baseline entries.
    Forget { path: PathBuf },
    /// Install (or remove) a periodic `scan` schedule via systemd or cron.
    Schedule {
        /// Remove the schedule instead of installing it.
        #[arg(long)]
        remove: bool,
        /// Force the systemd user-timer backend (default: auto-detect).
        #[arg(long, conflicts_with = "cron")]
        systemd: bool,
        /// Force the cron backend (default: auto-detect).
        #[arg(long)]
        cron: bool,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Run frequency.
        #[arg(long, value_enum, default_value_t = Interval::Hourly)]
        interval: Interval,
        /// Run time HH:MM. For `--interval daily` this is the run time; for
        /// hourly only the minute is used (run at that minute past each hour).
        #[arg(long, default_value = "09:00")]
        time: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Interval {
    Hourly,
    Daily,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_scan() {
        let cli = Cli::try_parse_from(["skillshield", "scan"]).unwrap();
        assert!(matches!(cli.command, Command::Scan { verbose: false }));
    }

    #[test]
    fn parses_scan_verbose() {
        let cli = Cli::try_parse_from(["skillshield", "scan", "-v"]).unwrap();
        assert!(matches!(cli.command, Command::Scan { verbose: true }));
    }

    #[test]
    fn parses_config() {
        let cli = Cli::try_parse_from(["skillshield", "config"]).unwrap();
        assert!(matches!(cli.command, Command::Config));
    }

    #[test]
    fn parses_forget_with_path() {
        let cli = Cli::try_parse_from(["skillshield", "forget", "/a/b"]).unwrap();
        match cli.command {
            Command::Forget { path } => assert_eq!(path, std::path::PathBuf::from("/a/b")),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_schedule_defaults() {
        let cli = Cli::try_parse_from(["skillshield", "schedule"]).unwrap();
        match cli.command {
            Command::Schedule {
                remove,
                interval,
                time,
                ..
            } => {
                assert!(!remove);
                assert_eq!(interval, Interval::Hourly);
                assert_eq!(time, "09:00");
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_trust_with_path() {
        let cli = Cli::try_parse_from(["skillshield", "trust", "/a/b"]).unwrap();
        match cli.command {
            Command::Trust { path } => assert_eq!(path, std::path::PathBuf::from("/a/b")),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn init_force_flag() {
        let cli = Cli::try_parse_from(["skillshield", "init", "--force"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Init {
                force: true,
                yes: false
            }
        ));
    }
}
