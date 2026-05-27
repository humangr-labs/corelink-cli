//! Config file at `~/.corelink/config.toml`.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default cache endpoint when none is configured.
pub const DEFAULT_ENDPOINT: &str = "https://corelink-api.humangr.com";
/// Default region selector.
pub const DEFAULT_REGION: &str = "auto";

/// Resolved CoreLink CLI config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Bearer token issued by the CoreLink control-plane.
    pub token: String,
    /// Region selector (`auto` lets the server pick).
    #[serde(default = "default_region")]
    pub region: String,
    /// Cache endpoint URL.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
}

fn default_region() -> String {
    DEFAULT_REGION.to_string()
}

fn default_endpoint() -> String {
    DEFAULT_ENDPOINT.to_string()
}

impl Config {
    /// Compute the path to `~/.corelink/config.toml`.
    pub fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not determine home directory")?;
        Ok(home.join(".corelink").join("config.toml"))
    }

    /// Load config from disk (or fail if missing).
    pub fn load() -> Result<Self> {
        let p = Self::path()?;
        let raw = fs::read_to_string(&p).with_context(|| {
            format!("config not found at {} — run `corelink login`", p.display())
        })?;
        Self::from_toml(&raw)
    }

    /// Load config, then apply optional overrides from CLI flags or env vars.
    ///
    /// If both `endpoint` and `token` are `Some`, the config file is not required
    /// (useful for one-shot CI invocations where no config file is present).
    pub fn load_with_overrides(
        endpoint: Option<String>,
        token: Option<String>,
    ) -> Result<Self> {
        // Attempt to load the base config; fall back to defaults when overrides
        // supply both fields so that CI callers need not create a config file.
        let mut cfg = match (endpoint.as_ref(), token.as_ref()) {
            (Some(_), Some(_)) => Self {
                token: String::new(),
                region: default_region(),
                endpoint: default_endpoint(),
            },
            _ => Self::load()?,
        };
        if let Some(ep) = endpoint {
            cfg.endpoint = ep;
        }
        if let Some(tok) = token {
            cfg.token = tok;
        }
        if cfg.token.is_empty() {
            anyhow::bail!(
                "no token found — pass --token, set CORELINK_TOKEN, or add `token` to ~/.corelink/config.toml"
            );
        }
        Ok(cfg)
    }

    /// Parse from a TOML string.
    pub fn from_toml(s: &str) -> Result<Self> {
        let cfg: Self = toml::from_str(s).context("invalid TOML in config")?;
        if cfg.token.is_empty() {
            anyhow::bail!("config.toml has empty `token`");
        }
        Ok(cfg)
    }

    /// Serialize to a TOML string.
    #[allow(dead_code)] // public API, exercised by tests; future `corelink login` will use it
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("failed to serialize config")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let cfg = Config::from_toml(r#"token = "abc123""#).unwrap();
        assert_eq!(cfg.token, "abc123");
        assert_eq!(cfg.region, DEFAULT_REGION);
        assert_eq!(cfg.endpoint, DEFAULT_ENDPOINT);
    }

    #[test]
    fn parse_full() {
        let cfg = Config::from_toml(
            r#"
token = "tok"
region = "us-east-1"
endpoint = "https://x.example.com"
"#,
        )
        .unwrap();
        assert_eq!(cfg.region, "us-east-1");
        assert_eq!(cfg.endpoint, "https://x.example.com");
    }

    #[test]
    fn roundtrip() {
        let cfg = Config {
            token: "tok".to_string(),
            region: "eu-west-1".to_string(),
            endpoint: "https://e.example.com".to_string(),
        };
        let s = cfg.to_toml().unwrap();
        let back = Config::from_toml(&s).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn empty_token_rejected() {
        assert!(Config::from_toml(r#"token = """#).is_err());
    }
}
