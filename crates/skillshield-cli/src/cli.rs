use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "skillshield", version, about = "Tripwire for AI-agent config artifacts")]
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
    },
    /// Scan and report changes vs. the baseline (scheduled use). Read-only.
    Scan,
    /// Show current changes vs. the baseline. Read-only.
    Status,
    /// Interactively accept/reject pending changes.
    Review,
    /// Accept a specific path into the baseline.
    Trust {
        path: PathBuf,
    },
    /// Add a project root: crawl once, record in config, trust findings.
    Monitor {
        path: PathBuf,
    },
    /// Remove a project root from config and prune its baseline entries.
    Unmonitor {
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_scan() {
        let cli = Cli::try_parse_from(["skillshield", "scan"]).unwrap();
        assert!(matches!(cli.command, Command::Scan));
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
        assert!(matches!(cli.command, Command::Init { force: true }));
    }
}
