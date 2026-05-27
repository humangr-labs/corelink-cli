//! `corelink ping` — health-check the configured cache endpoint.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config::Config;

/// Run the `ping` subcommand.
pub fn run() -> Result<()> {
    let cfg = Config::load().context("failed to load ~/.corelink/config.toml")?;
    let url = format!("{}/v1/ping", cfg.endpoint.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")?;

    let started = Instant::now();
    let resp = client
        .get(&url)
        .bearer_auth(&cfg.token)
        .send()
        .with_context(|| format!("request to {url} failed"))?;
    let elapsed = started.elapsed();

    let status = resp.status();
    if status.is_success() {
        println!("Cache reachable in {} ms", elapsed.as_millis());
        Ok(())
    } else {
        anyhow::bail!("ping failed: HTTP {status} from {url}");
    }
}
