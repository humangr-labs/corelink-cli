//! `corelink docker-init` — configure Docker BuildKit remote registry cache.
//!
//! Writes `.docker/buildx-cache.json` with BuildKit `--cache-from` / `--cache-to`
//! configuration pointing at the CoreLink OCI registry cache layer.
//! Appends `.docker/buildx-cache.json` to `.gitignore` (the file contains a
//! token and must not be committed).
//!
//! Detection: `Dockerfile` must be present in the current directory.
//!
//! Idempotency: if `.docker/buildx-cache.json` already exists and its
//! `remoteCache.ref` matches the configured endpoint, emits a noisy log and
//! exits 0 without modifying any file.
//!
//! Vendor docs: https://docs.docker.com/build/cache/backends/registry/

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::config::Config;

const GITIGNORE_MARKER_BEGIN: &str = "# corelink-managed (do not edit between markers)";
const GITIGNORE_MARKER_END: &str = "# /corelink-managed";
const GITIGNORE_LINE: &str = ".docker/buildx-cache.json";

/// Arguments for `docker-init`.
#[derive(Debug, Args)]
pub struct DockerInitArgs {
    /// CoreLink cache endpoint URL (overrides config and env).
    #[arg(long, env = "CORELINK_ENDPOINT")]
    pub endpoint: Option<String>,

    /// Bearer token (overrides config and env).
    #[arg(long, env = "CORELINK_TOKEN")]
    pub token: Option<String>,
}

/// Run the `docker-init` subcommand.
pub fn run(args: DockerInitArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("cwd unreadable")?;

    if !has_dockerfile(&cwd) {
        anyhow::bail!(
            "no Dockerfile found in {} — run `corelink docker-init` from the project root",
            cwd.display()
        );
    }

    let cfg = Config::load_with_overrides(args.endpoint, args.token)
        .context("failed to load config")?;

    let docker_dir = cwd.join(".docker");
    let cache_path = docker_dir.join("buildx-cache.json");
    let gitignore_path = cwd.join(".gitignore");

    // Check idempotency.
    if let Ok(existing) = fs::read_to_string(&cache_path) {
        if is_already_applied(&existing, &cfg.endpoint) {
            println!(
                "corelink: .docker/buildx-cache.json already configured for {} (no-op)",
                cfg.endpoint
            );
            return Ok(());
        }
    }

    // Create .docker/ dir.
    fs::create_dir_all(&docker_dir)
        .with_context(|| format!("failed to create {}", docker_dir.display()))?;

    // Write buildx-cache.json.
    let cache_json = render_cache_json(&cfg.endpoint, &cfg.token);
    fs::write(&cache_path, cache_json)
        .context("failed to write .docker/buildx-cache.json")?;
    println!("corelink: wrote {}", cache_path.display());

    // Append .docker/buildx-cache.json to .gitignore so the token isn't committed.
    append_gitignore(&gitignore_path)?;
    println!("corelink: updated {}", gitignore_path.display());

    println!(
        "corelink: BuildKit registry cache enabled → {}/cache",
        cfg.endpoint.trim_end_matches('/')
    );
    println!(
        "corelink: inject into your docker build with:\n\
         \t--cache-from type=registry,ref={ep}/cache \\\n\
         \t--cache-to   type=registry,ref={ep}/cache,mode=max",
        ep = cfg.endpoint.trim_end_matches('/')
    );

    Ok(())
}

fn has_dockerfile(dir: &Path) -> bool {
    dir.join("Dockerfile").exists()
}

/// Check whether `.docker/buildx-cache.json` already points at the given endpoint.
pub fn is_already_applied(existing_json: &str, endpoint: &str) -> bool {
    let endpoint = endpoint.trim_end_matches('/');
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(existing_json) {
        if let Some(rc) = v.get("remoteCache") {
            if let Some(existing_ref) = rc.get("ref").and_then(|r| r.as_str()) {
                let existing_ep = existing_ref
                    .strip_suffix("/cache")
                    .unwrap_or(existing_ref)
                    .trim_end_matches('/');
                return existing_ep == endpoint;
            }
        }
    }
    false
}

fn render_cache_json(endpoint: &str, token: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    let v = serde_json::json!({
        "remoteCache": {
            "type": "registry",
            "ref": format!("{}/cache", endpoint),
            "auth": {
                "type": "bearer",
                "token": token
            }
        },
        "buildkit": {
            "cacheFrom": format!("type=registry,ref={}/cache", endpoint),
            "cacheTo": format!("type=registry,ref={}/cache,mode=max", endpoint)
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

/// Append the `.docker/buildx-cache.json` line inside a corelink-managed
/// block in `.gitignore`.
fn append_gitignore(gitignore_path: &Path) -> Result<()> {
    let existing = match fs::read_to_string(gitignore_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("failed to read .gitignore"),
    };

    // Check idempotency.
    if existing.contains(GITIGNORE_LINE) {
        return Ok(());
    }

    let block = format!(
        "{GITIGNORE_MARKER_BEGIN}\n{GITIGNORE_LINE}\n{GITIGNORE_MARKER_END}\n"
    );
    let updated = upsert_text_block(&existing, &block);
    fs::write(gitignore_path, updated).context("failed to write .gitignore")
}

/// Replace the corelink-managed block if present, else append it.
pub fn upsert_text_block(existing: &str, new_block: &str) -> String {
    if let (Some(begin), Some(end)) = (
        existing.find(GITIGNORE_MARKER_BEGIN),
        existing.find(GITIGNORE_MARKER_END),
    ) {
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
    fn detect_success_finds_dockerfile() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Dockerfile"), "FROM debian:bookworm-slim\n").unwrap();
        assert!(has_dockerfile(dir.path()));
    }

    // ── detect-miss ──────────────────────────────────────────────────────────

    #[test]
    fn detect_miss_no_dockerfile() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_dockerfile(dir.path()));
    }

    // ── already-applied noop ─────────────────────────────────────────────────

    #[test]
    fn already_applied_noop_same_endpoint() {
        let endpoint = "https://corelink-api.humangr.com";
        let token = "ct_testtoken";
        let json = render_cache_json(endpoint, token);
        assert!(
            is_already_applied(&json, endpoint),
            "written config should be detected as already applied"
        );
    }

    #[test]
    fn already_applied_false_for_different_endpoint() {
        let json = render_cache_json("https://corelink-api.humangr.com", "tok");
        assert!(
            !is_already_applied(&json, "https://other.example.com"),
            "different endpoint must not be detected as already applied"
        );
    }

    // ── buildx-cache.json structure ──────────────────────────────────────────

    #[test]
    fn cache_json_contains_required_keys() {
        let json = render_cache_json("https://corelink-api.humangr.com", "ct_tok");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["remoteCache"]["type"], "registry");
        assert_eq!(
            v["remoteCache"]["ref"],
            "https://corelink-api.humangr.com/cache"
        );
        assert_eq!(v["remoteCache"]["auth"]["type"], "bearer");
        assert_eq!(v["remoteCache"]["auth"]["token"], "ct_tok");
        assert!(v["buildkit"]["cacheFrom"]
            .as_str()
            .unwrap()
            .starts_with("type=registry,ref="));
    }

    // ── gitignore upsert ─────────────────────────────────────────────────────

    #[test]
    fn gitignore_block_appended_when_absent() {
        let existing = "node_modules/\n.env\n";
        let block = format!(
            "{GITIGNORE_MARKER_BEGIN}\n{GITIGNORE_LINE}\n{GITIGNORE_MARKER_END}\n"
        );
        let updated = upsert_text_block(existing, &block);
        assert!(updated.contains(GITIGNORE_LINE));
        assert!(updated.starts_with("node_modules/"));
    }

    #[test]
    fn gitignore_block_idempotent() {
        let block = format!(
            "{GITIGNORE_MARKER_BEGIN}\n{GITIGNORE_LINE}\n{GITIGNORE_MARKER_END}\n"
        );
        let once = upsert_text_block("", &block);
        let twice = upsert_text_block(&once, &block);
        assert_eq!(once, twice);
    }
}
