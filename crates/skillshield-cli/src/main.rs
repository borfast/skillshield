mod cli;
mod commands;
mod exit;
mod review_ui;

use clap::Parser;

fn main() {
    let parsed = cli::Cli::parse();
    exit::finish(commands::run(parsed.command));
}
