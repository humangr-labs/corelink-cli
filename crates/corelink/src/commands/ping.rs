//! `corelink ping` — health-check the configured cache endpoint.
//!
//! Overrides (in priority order, highest first):
//!   1. `--endpoint` / `--token` CLI flags
//!   2. `CORELINK_ENDPOINT` / `CORELINK_TOKEN` env vars
//!   3. `~/.corelink/config.toml`

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Args;

use crate::config::Config;

/// Arguments for the `ping` subcommand.
#[derive(Debug, Args)]
pub struct PingArgs {
    /// Cache endpoint URL (overrides config and env).
    #[arg(long, env = "CORELINK_ENDPOINT")]
    pub endpoint: Option<String>,

    /// Bearer token (overrides config and env).
    #[arg(long, env = "CORELINK_TOKEN")]
    pub token: Option<String>,
}

/// Run the `ping` subcommand.
pub fn run(args: PingArgs) -> Result<()> {
    let cfg = Config::load_with_overrides(args.endpoint, args.token)
        .context("failed to load config")?;
    let url = format!("{}/v1/ping", cfg.endpoint.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")?;

    let started = Instant::now();
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.token)
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .body("")
        .send()
        .with_context(|| format!("POST {url} failed"))?;
    let elapsed = started.elapsed();

    let status = resp.status();
    println!("POST {url} → {status} ({} ms)", elapsed.as_millis());
    if status.is_success() {
        Ok(())
    } else {
        anyhow::bail!("ping failed: HTTP {status} from {url}");
    }
}
