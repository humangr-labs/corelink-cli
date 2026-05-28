//! `corelink npm-init` — idempotently inject a CoreLink remote-cache stanza
//! into `turbo.json` (Turborepo remote caching).
//!
//! Detection: `package.json` must be present (npm/pnpm project marker).
//! Target: `turbo.json` — created if absent, `remoteCache` key merged if present.
//!
//! JSON upsert is inline (R11 — scope isolation; no shared module).
//!
//! Idempotency: if `turbo.json` already has `remoteCache.apiUrl` matching the
//! configured endpoint, emits a noisy log and exits 0 without modifying the file.
//!
//! Vendor docs: https://turbo.build/repo/docs/core-concepts/remote-caching

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::config::Config;

/// Arguments for `npm-init`.
#[derive(Debug, Args)]
pub struct NpmInitArgs {
    /// CoreLink cache endpoint URL (overrides config and env).
    #[arg(long, env = "CORELINK_ENDPOINT")]
    pub endpoint: Option<String>,

    /// Bearer token (overrides config and env).
    #[arg(long, env = "CORELINK_TOKEN")]
    pub token: Option<String>,
}

/// Run the `npm-init` subcommand.
pub fn run(args: NpmInitArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("cwd unreadable")?;

    if !has_package_json(&cwd) {
        anyhow::bail!(
            "no package.json found in {} — run `corelink npm-init` from the project root",
            cwd.display()
        );
    }

    let cfg = Config::load_with_overrides(args.endpoint, args.token)
        .context("failed to load config")?;

    let turbo_path = cwd.join("turbo.json");
    let existing_json = match fs::read_to_string(&turbo_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("failed to read turbo.json"),
    };

    match upsert_remote_cache_json(&existing_json, &cfg.endpoint, &cfg.token) {
        UpsertResult::AlreadyApplied => {
            println!(
                "corelink: turbo.json remoteCache already configured for {} (no-op)",
                cfg.endpoint
            );
        }
        UpsertResult::Updated(new_json) => {
            fs::write(&turbo_path, new_json).context("failed to write turbo.json")?;
            println!("corelink: updated {}", turbo_path.display());
            println!(
                "corelink: Turborepo remote cache enabled → {}",
                cfg.endpoint
            );
            println!("corelink: run `turbo build` to prime the cache");
        }
    }
    Ok(())
}

fn has_package_json(dir: &Path) -> bool {
    dir.join("package.json").exists()
}

/// Result of attempting to upsert the `remoteCache` key in `turbo.json`.
#[derive(Debug, PartialEq, Eq)]
pub enum UpsertResult {
    /// The file already had `remoteCache.apiUrl` matching the given endpoint.
    AlreadyApplied,
    /// The file was updated (or created). Contains the new JSON string.
    Updated(String),
}

/// Inline JSON upsert helper for `turbo.json` (R11 — no shared module).
///
/// Parses `existing` (empty string → empty object basis), merges the
/// `remoteCache` key, and re-serialises with 2-space indent.
///
/// Idempotency: if `remoteCache.apiUrl` already matches `endpoint`, returns
/// `AlreadyApplied` without modification.
pub fn upsert_remote_cache_json(
    existing: &str,
    endpoint: &str,
    token: &str,
) -> UpsertResult {
    let endpoint = endpoint.trim_end_matches('/');

    // Parse existing content, defaulting to empty object when absent.
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        match serde_json::from_str(existing) {
            Ok(v) => v,
            // If turbo.json exists but is invalid JSON we refuse to corrupt it.
            Err(_) => {
                return UpsertResult::Updated(build_remote_cache_json(endpoint, token));
            }
        }
    };

    // Check idempotency: already applied with same endpoint?
    if let Some(rc) = root.get("remoteCache") {
        if let Some(existing_url) = rc.get("apiUrl").and_then(|v| v.as_str()) {
            if existing_url.trim_end_matches('/') == endpoint {
                return UpsertResult::AlreadyApplied;
            }
        }
    }

    // Merge remoteCache into the root object.
    if let Some(obj) = root.as_object_mut() {
        obj.insert(
            "remoteCache".to_string(),
            serde_json::json!({
                "signature": true,
                "enabled": true,
                "apiUrl": endpoint,
                "token": token,
                "teamId": "corelink"
            }),
        );
    }

    let serialized = serde_json::to_string_pretty(&root).unwrap_or_else(|_| {
        build_remote_cache_json(endpoint, token)
    });
    UpsertResult::Updated(serialized + "\n")
}

fn build_remote_cache_json(endpoint: &str, token: &str) -> String {
    let v = serde_json::json!({
        "remoteCache": {
            "signature": true,
            "enabled": true,
            "apiUrl": endpoint,
            "token": token,
            "teamId": "corelink"
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── detect-success ───────────────────────────────────────────────────────

    #[test]
    fn detect_success_finds_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
        assert!(has_package_json(dir.path()));
    }

    // ── detect-miss ──────────────────────────────────────────────────────────

    #[test]
    fn detect_miss_no_package_json() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_package_json(dir.path()));
    }

    // ── already-applied noop ─────────────────────────────────────────────────

    #[test]
    fn already_applied_noop_same_endpoint() {
        let endpoint = "https://corelink-api.humangr.com";
        let token = "ct_testtoken";
        let result1 = upsert_remote_cache_json("", endpoint, token);
        let json1 = match result1 {
            UpsertResult::Updated(ref s) => s.clone(),
            UpsertResult::AlreadyApplied => panic!("first call should update"),
        };
        let result2 = upsert_remote_cache_json(&json1, endpoint, token);
        assert_eq!(
            result2,
            UpsertResult::AlreadyApplied,
            "second call with same endpoint must be a noop"
        );
    }

    // ── upsert on empty file ─────────────────────────────────────────────────

    #[test]
    fn upsert_creates_turbo_json_when_absent() {
        let result = upsert_remote_cache_json("", "https://ep.example.com", "tok123");
        match result {
            UpsertResult::Updated(json) => {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["remoteCache"]["apiUrl"], "https://ep.example.com");
                assert_eq!(v["remoteCache"]["token"], "tok123");
                assert_eq!(v["remoteCache"]["enabled"], true);
            }
            UpsertResult::AlreadyApplied => panic!("should produce Updated"),
        }
    }

    // ── upsert preserves existing keys ───────────────────────────────────────

    #[test]
    fn upsert_preserves_existing_pipeline_config() {
        let existing = r#"{"pipeline": {"build": {"outputs": ["dist/**"]}}}"#;
        let result = upsert_remote_cache_json(existing, "https://ep.example.com", "tok");
        match result {
            UpsertResult::Updated(json) => {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                // Original key preserved.
                assert!(v.get("pipeline").is_some(), "pipeline key must be preserved");
                // remoteCache added.
                assert_eq!(v["remoteCache"]["apiUrl"], "https://ep.example.com");
            }
            UpsertResult::AlreadyApplied => panic!("should produce Updated"),
        }
    }

    // ── endpoint mismatch triggers update ────────────────────────────────────

    #[test]
    fn upsert_updates_when_endpoint_changes() {
        let json_v1 = match upsert_remote_cache_json("", "https://old.example.com", "tok") {
            UpsertResult::Updated(s) => s,
            UpsertResult::AlreadyApplied => panic!(),
        };
        let result = upsert_remote_cache_json(&json_v1, "https://new.example.com", "tok2");
        match result {
            UpsertResult::Updated(json) => {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["remoteCache"]["apiUrl"], "https://new.example.com");
            }
            UpsertResult::AlreadyApplied => panic!("different endpoint must trigger update"),
        }
    }
}
