//! `corelink config show` — print resolved config (token redacted).

use anyhow::Result;

use crate::config::Config;

/// Run the `config show` subcommand.
pub fn run() -> Result<()> {
    let cfg = Config::load()?;
    println!("endpoint = {}", cfg.endpoint);
    println!("region   = {}", cfg.region);
    println!("token    = {}", redact(&cfg.token));
    Ok(())
}

fn redact(token: &str) -> String {
    if token.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}…{}", &token[..4], &token[token.len() - 4..])
    }
}
