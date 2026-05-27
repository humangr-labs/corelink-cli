//! CoreLink CLI entrypoint.
//!
//! Subcommands:
//! - `ping`         — health-check the configured cache endpoint
//! - `bazel-init`   — inject `.bazelrc` remote-cache stanza idempotently
//! - `config show`  — print the resolved config from `~/.corelink/config.toml`

#![forbid(unsafe_code)]

mod commands;
mod config;

use clap::{Parser, Subcommand};

use commands::ping::PingArgs;

/// CoreLink CLI — shared content-addressable cache client.
#[derive(Debug, Parser)]
#[command(name = "corelink", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Health-check the configured cache endpoint (POSTs to /v1/ping).
    Ping(PingArgs),
    /// Inject `.bazelrc` remote-cache stanza (idempotent).
    BazelInit,
    /// Config inspection.
    #[command(subcommand)]
    Config(ConfigCmd),
}

#[derive(Debug, Subcommand)]
enum ConfigCmd {
    /// Print the resolved config (token redacted).
    Show,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ping(args) => commands::ping::run(args),
        Command::BazelInit => commands::bazel_init::run(),
        Command::Config(ConfigCmd::Show) => commands::config_show::run(),
    }
}
