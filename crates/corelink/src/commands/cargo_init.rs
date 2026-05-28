//! `corelink cargo-init` — idempotently inject sccache remote-cache config into `.cargo/config.toml`.
//!
//! Writes to the project-local `.cargo/config.toml` (NOT the user-global
//! `~/.cargo/config.toml`) so that the change is scoped to this workspace only.
//!
//! Prerequisites checked at runtime:
//! - `Cargo.toml` must be present in the current directory (project marker).
//! - `sccache` binary must be on PATH (CoreLink uses sccache as the cargo-side
//!   cache wrapper). If absent, an actionable install hint is emitted and the
//!   command exits with an error.
//!
//! Resolution notes:
//! - R9: project-local `.cargo/config.toml`, NOT user-global.
//! - R26: sccache presence check; no auto-install.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use clap::Args;

use crate::config::Config;

const MARKER_BEGIN: &str = "# corelink-managed (do not edit between markers)";
const MARKER_END: &str = "# /corelink-managed";

/// Arguments for `cargo-init`.
#[derive(Debug, Args)]
pub struct CargoInitArgs {
    /// CoreLink cache endpoint URL (overrides config and env).
    #[arg(long, env = "CORELINK_ENDPOINT")]
    pub endpoint: Option<String>,

    /// Bearer token (overrides config and env).
    #[arg(long, env = "CORELINK_TOKEN")]
    pub token: Option<String>,
}

/// Run the `cargo-init` subcommand.
pub fn run(args: CargoInitArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("cwd unreadable")?;

    // Detect project marker.
    if !has_cargo_toml(&cwd) {
        anyhow::bail!(
            "no Cargo.toml found in {} — run `corelink cargo-init` from the project root",
            cwd.display()
        );
    }

    // R26: check sccache is installed.
    check_sccache()?;

    let cfg = Config::load_with_overrides(args.endpoint, args.token)
        .context("failed to load config")?;

    let cargo_dir = cwd.join(".cargo");
    fs::create_dir_all(&cargo_dir)
        .with_context(|| format!("failed to create {}", cargo_dir.display()))?;

    let config_path = cargo_dir.join("config.toml");
    let existing = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("failed to read .cargo/config.toml"),
    };

    let stanza = render_stanza(&cfg.endpoint, &cfg.token);
    let updated = upsert_block(&existing, &stanza);
    if updated == existing {
        println!("corelink: .cargo/config.toml already up-to-date (no-op)");
    } else {
        fs::write(&config_path, updated)
            .context("failed to write .cargo/config.toml")?;
        println!("corelink: updated {}", config_path.display());
        println!(
            "corelink: sccache will proxy Cargo builds to CoreLink at {}",
            cfg.endpoint
        );
        println!("corelink: run `cargo build` — sccache intercepts rustc calls automatically");
    }
    Ok(())
}

fn has_cargo_toml(dir: &Path) -> bool {
    dir.join("Cargo.toml").exists()
}

/// Check that `sccache` is available on PATH (R26).
///
/// Does NOT auto-install. Emits an actionable hint on failure.
fn check_sccache() -> Result<()> {
    let ok = Command::new("sccache")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !ok {
        anyhow::bail!(
            "sccache binary not found on PATH.\n\
             CoreLink uses sccache as the cargo-side cache wrapper.\n\
             Install via one of:\n\
             \n\
             \tcargo install sccache --locked\n\
             \tbrew install sccache          (macOS)\n\
             \tapt-get install sccache       (Debian/Ubuntu)\n\
             \n\
             Then re-run `corelink cargo-init`."
        );
    }
    Ok(())
}

fn render_stanza(endpoint: &str, token: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    format!(
        "{MARKER_BEGIN}\n\
         [build]\n\
         rustc-wrapper = \"sccache\"\n\
         \n\
         [env]\n\
         SCCACHE_ENDPOINT = \"{endpoint}/cargo\"\n\
         SCCACHE_BUCKET = \"corelink\"\n\
         SCCACHE_AUTH_TYPE = \"bearer\"\n\
         SCCACHE_AUTH_TOKEN = \"{token}\"\n\
         {MARKER_END}\n"
    )
}

/// Replace the corelink-managed block if present, else append it.
///
/// Shared pattern from `bazel_init.rs` — re-implemented here to keep each
/// command module self-contained (R9/R11 scope isolation).
pub fn upsert_block(existing: &str, new_block: &str) -> String {
    if let (Some(begin), Some(end)) = (existing.find(MARKER_BEGIN), existing.find(MARKER_END)) {
        if end > begin {
            let end_line_end = existing[end..]
                .find('\n')
                .map_or(existing.len(), |off| end + off + 1);
            let mut out = String::with_capacity(existing.len() + new_block.len());
            out.push_str(&existing[..begin]);
            out.push_str(new_block);
            out.push_str(&existing[end_line_end..]);
            return out;
        }
    }
    let mut out = String::with_capacity(existing.len() + new_block.len() + 1);
    out.push_str(existing);
    if !existing.is_empty() && !existing.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(new_block);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── detect-success ───────────────────────────────────────────────────────

    #[test]
    fn detect_success_finds_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert!(has_cargo_toml(dir.path()));
    }

    // ── detect-miss ──────────────────────────────────────────────────────────

    #[test]
    fn detect_miss_no_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_cargo_toml(dir.path()));
    }

    // ── idempotency (already-applied-noop) ───────────────────────────────────

    #[test]
    fn already_applied_noop() {
        let stanza = render_stanza("https://api.example.com", "tok");
        let once = upsert_block("", &stanza);
        let twice = upsert_block(&once, &stanza);
        assert_eq!(once, twice, "second upsert must be a noop");
    }

    // ── upsert mechanics ─────────────────────────────────────────────────────

    #[test]
    fn upsert_appends_when_absent() {
        let existing = "[profile.release]\nlto = \"thin\"\n";
        let stanza = render_stanza("https://ep", "t");
        let out = upsert_block(existing, &stanza);
        assert!(out.starts_with("[profile.release]"));
        assert!(out.contains("corelink-managed"));
        assert!(out.contains("sccache"));
        assert!(out.contains("https://ep/cargo"));
    }

    #[test]
    fn upsert_replaces_existing_block() {
        let old_stanza = render_stanza("https://old.ep", "old-tok");
        let with_old = upsert_block("", &old_stanza);
        let new_stanza = render_stanza("https://new.ep", "new-tok");
        let with_new = upsert_block(&with_old, &new_stanza);
        assert!(with_new.contains("https://new.ep/cargo"));
        assert!(!with_new.contains("https://old.ep/cargo"));
    }

    #[test]
    fn stanza_contains_required_keys() {
        let stanza = render_stanza("https://corelink-api.humangr.com", "ct_testtoken");
        assert!(stanza.contains("rustc-wrapper = \"sccache\""));
        assert!(stanza.contains("SCCACHE_ENDPOINT"));
        assert!(stanza.contains("SCCACHE_AUTH_TOKEN"));
        assert!(stanza.contains("ct_testtoken"));
    }
}
