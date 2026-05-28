//! CoreLink CLI entrypoint.
//!
//! Subcommands:
//! - `ping`         — health-check the configured cache endpoint
//! - `bazel-init`   — inject `.bazelrc` remote-cache stanza idempotently
//! - `cargo-init`   — inject `.cargo/config.toml` sccache stanza idempotently
//! - `npm-init`     — inject `turbo.json` remoteCache stanza idempotently
//! - `docker-init`  — configure Docker BuildKit remote registry cache
//! - `doctor`       — 9-point environment diagnostic
//! - `config show`  — print the resolved config from `~/.corelink/config.toml`

#![forbid(unsafe_code)]

mod commands;
mod config;

use clap::{Parser, Subcommand};

use commands::cargo_init::CargoInitArgs;
use commands::docker_init::DockerInitArgs;
use commands::doctor::DoctorArgs;
use commands::npm_init::NpmInitArgs;
use commands::ping::PingArgs;

/// CoreLink CLI — shared content-addressable cache client.
#[derive(Debug, Parser)]
#[command(name = "corelink", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[non_exhaustive]
#[derive(Debug, Subcommand)]
enum Command {
    /// Health-check the configured cache endpoint (POSTs to /v1/ping).
    Ping(PingArgs),
    /// Inject `.bazelrc` remote-cache stanza (idempotent).
    BazelInit,
    /// Inject project-local `.cargo/config.toml` sccache stanza (idempotent).
    ///
    /// Requires `sccache` on PATH. Writes to `.cargo/config.toml` in the current
    /// directory — NOT the user-global `~/.cargo/config.toml`.
    CargoInit(CargoInitArgs),
    /// Inject `turbo.json` remoteCache stanza for Turborepo (idempotent).
    ///
    /// Creates `turbo.json` if absent, merges `remoteCache` key if already present.
    NpmInit(NpmInitArgs),
    /// Configure Docker BuildKit remote registry cache (idempotent).
    ///
    /// Writes `.docker/buildx-cache.json` and appends it to `.gitignore`.
    DockerInit(DockerInitArgs),
    /// 9-point environment diagnostic: auth, endpoint, installed tool deps.
    ///
    /// Exits 0 if all checks pass, 1 if any fail.
    Doctor(DoctorArgs),
    /// Config inspection.
    #[command(subcommand)]
    Config(ConfigCmd),
}

#[non_exhaustive]
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
        Command::CargoInit(args) => commands::cargo_init::run(args),
        Command::NpmInit(args) => commands::npm_init::run(args),
        Command::DockerInit(args) => commands::docker_init::run(args),
        Command::Doctor(args) => commands::doctor::run(args),
        Command::Config(ConfigCmd::Show) => commands::config_show::run(),
    }
}
