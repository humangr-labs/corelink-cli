//! `corelink doctor` — 9-point environment diagnostic.
//!
//! Checks the local environment and connectivity in order, printing a
//! status table of ✓ (pass) / ✗ (fail) with actionable hints for each
//! failure. Follows the UX of `gh auth status`, `rustup check`, and
//! `brew doctor`.
//!
//! Exit codes:
//! - 0: all checks pass
//! - 1: at least one check failed
//!
//! 9-point checklist (per R1 user decision):
//!  1. CLI version + build target                (always ✓)
//!  2. `CORELINK_ENDPOINT` present + URL valid
//!  3. `CORELINK_TOKEN` present + format `ct_` prefix
//!  4. Endpoint reachable (HTTP HEAD/GET, 5 s timeout)
//!  5. Auth valid (POST /v1/ping → 200, not 401/403)
//!  6. If bazel-init was run (.bazelrc corelink block), verify `bazel` on PATH
//!  7. If cargo-init was run (.cargo/config.toml corelink block), verify `sccache`
//!  8. If npm-init was run (turbo.json has remoteCache), verify `turbo`
//!  9. If docker-init was run (.docker/buildx-cache.json present), verify `docker buildx`

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Args;

use crate::config::{Config, DEFAULT_ENDPOINT};

/// Arguments for `doctor`.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Print HTTP request/response details and full env-var values
    /// (token is always REDACTED even in verbose mode).
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

/// Outcome of a single doctor check.
#[derive(Debug)]
struct Check {
    label: &'static str,
    passed: bool,
    detail: String,
}

impl Check {
    fn pass(label: &'static str, detail: impl Into<String>) -> Self {
        Self { label, passed: true, detail: detail.into() }
    }

    fn fail(label: &'static str, detail: impl Into<String>) -> Self {
        Self { label, passed: false, detail: detail.into() }
    }
}

/// Run the `doctor` subcommand.
pub fn run(args: DoctorArgs) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    let mut checks: Vec<Check> = Vec::with_capacity(9);
    let mut all_pass = true;

    // ── Check 1: CLI version + build target ──────────────────────────────────
    checks.push(Check::pass(
        "CLI version",
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH
        ),
    ));

    // ── Resolve endpoint + token (without failing on missing config file) ────
    let (endpoint, token) = resolve_endpoint_token();

    // ── Check 2: CORELINK_ENDPOINT present + URL valid ───────────────────────
    {
        let ep = std::env::var("CORELINK_ENDPOINT")
            .ok()
            .unwrap_or_default();
        if ep.is_empty() {
            // Fall back to config file or hardcoded default.
            let using_default = endpoint == DEFAULT_ENDPOINT;
            if using_default {
                checks.push(Check::pass(
                    "endpoint",
                    format!("{} (default — set CORELINK_ENDPOINT to override)", endpoint),
                ));
            } else {
                checks.push(Check::pass(
                    "endpoint",
                    format!("{} (from ~/.corelink/config.toml)", endpoint),
                ));
            }
        } else if is_valid_url(&ep) {
            checks.push(Check::pass("endpoint", format!("{ep} (env)")));
        } else {
            checks.push(Check::fail(
                "endpoint",
                format!(
                    "CORELINK_ENDPOINT=\"{ep}\" is not a valid URL; \
                     expected https://... scheme"
                ),
            ));
        }
    }

    // ── Check 3: CORELINK_TOKEN present + ct_ prefix ─────────────────────────
    {
        let env_token = std::env::var("CORELINK_TOKEN").ok().unwrap_or_default();
        let effective_token = if !env_token.is_empty() {
            env_token.clone()
        } else {
            token.clone()
        };

        if effective_token.is_empty() {
            checks.push(Check::fail(
                "token",
                "no token found — pass --token, set CORELINK_TOKEN, \
                 or add `token` to ~/.corelink/config.toml"
                    .to_string(),
            ));
        } else if !effective_token.starts_with("ct_") {
            checks.push(Check::fail(
                "token",
                format!(
                    "token does not start with `ct_` — \
                     CoreLink PATs are prefixed `ct_`; \
                     got: {}... (check for stale/wrong token)",
                    redact(&effective_token, 6)
                ),
            ));
        } else {
            let source = if !env_token.is_empty() {
                "env"
            } else {
                "~/.corelink/config.toml"
            };
            checks.push(Check::pass(
                "token",
                format!("{}... ({})", redact(&effective_token, 8), source),
            ));
        }
    }

    // ── Check 4 + 5: endpoint reachable + auth valid ─────────────────────────
    // Only attempt if we have an endpoint + token.
    if !endpoint.is_empty() && !token.is_empty() {
        let url = format!("{}/v1/ping", endpoint.trim_end_matches('/'));
        let (reach_ok, reach_detail, auth_ok, auth_detail, elapsed_ms) =
            probe_endpoint(&url, &token, args.verbose);

        checks.push(if reach_ok {
            Check::pass(
                "endpoint reachable",
                format!("POST {url} ({elapsed_ms} ms)"),
            )
        } else {
            Check::fail("endpoint reachable", reach_detail)
        });

        checks.push(if auth_ok {
            Check::pass("auth valid", format!("HTTP 200 ({elapsed_ms} ms)"))
        } else {
            Check::fail("auth valid", auth_detail)
        });
    } else {
        checks.push(Check::fail(
            "endpoint reachable",
            "skipped — endpoint or token not configured".to_string(),
        ));
        checks.push(Check::fail(
            "auth valid",
            "skipped — endpoint or token not configured".to_string(),
        ));
    }

    // ── Check 6: bazel-init → bazel on PATH ──────────────────────────────────
    {
        let bazelrc = cwd.join(".bazelrc");
        if file_contains(&bazelrc, "corelink-managed") {
            if bin_on_path("bazel") {
                checks.push(Check::pass(
                    "bazel (bazel-init applied)",
                    "bazel found on PATH",
                ));
            } else {
                checks.push(Check::fail(
                    "bazel (bazel-init applied)",
                    "bazel not found on PATH — install Bazel ≥ 7 from https://bazel.build/install"
                        .to_string(),
                ));
            }
        } else {
            checks.push(Check::pass(
                "bazel (bazel-init not applied)",
                "skipped — no .bazelrc corelink block detected".to_string(),
            ));
        }
    }

    // ── Check 7: cargo-init → sccache on PATH ────────────────────────────────
    {
        let cargo_cfg = cwd.join(".cargo").join("config.toml");
        if file_contains(&cargo_cfg, "corelink-managed") {
            if bin_on_path("sccache") {
                checks.push(Check::pass(
                    "sccache (cargo-init applied)",
                    "sccache found on PATH",
                ));
            } else {
                checks.push(Check::fail(
                    "sccache (cargo-init applied)",
                    "sccache not found — install via: cargo install sccache --locked\n\
                     \t  or: brew install sccache"
                        .to_string(),
                ));
            }
        } else {
            checks.push(Check::pass(
                "sccache (cargo-init not applied)",
                "skipped — no .cargo/config.toml corelink block detected".to_string(),
            ));
        }
    }

    // ── Check 8: npm-init → turbo on PATH ────────────────────────────────────
    {
        let turbo_json = cwd.join("turbo.json");
        if file_contains(&turbo_json, "remoteCache") {
            if bin_on_path("turbo") {
                checks.push(Check::pass(
                    "turbo (npm-init applied)",
                    "turbo found on PATH",
                ));
            } else {
                checks.push(Check::fail(
                    "turbo (npm-init applied)",
                    "turbo not found — install via: npm install -g turbo\n\
                     \t  or: npx turbo (no global install)"
                        .to_string(),
                ));
            }
        } else {
            checks.push(Check::pass(
                "turbo (npm-init not applied)",
                "skipped — no turbo.json remoteCache block detected".to_string(),
            ));
        }
    }

    // ── Check 9: docker-init → docker buildx ─────────────────────────────────
    {
        let buildx_cfg = cwd.join(".docker").join("buildx-cache.json");
        if buildx_cfg.exists() {
            // Check `docker buildx version`.
            let ok = Command::new("docker")
                .args(["buildx", "version"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if ok {
                checks.push(Check::pass(
                    "docker buildx (docker-init applied)",
                    "docker buildx available",
                ));
            } else {
                checks.push(Check::fail(
                    "docker buildx (docker-init applied)",
                    "docker buildx not available — install Docker Desktop ≥ 4 \
                     or the buildx CLI plugin: https://docs.docker.com/buildx/install"
                        .to_string(),
                ));
            }
        } else {
            checks.push(Check::pass(
                "docker buildx (docker-init not applied)",
                "skipped — no .docker/buildx-cache.json detected".to_string(),
            ));
        }
    }

    // ── Print results table ───────────────────────────────────────────────────
    println!("corelink doctor — environment diagnostic\n");
    for (i, check) in checks.iter().enumerate() {
        let icon = if check.passed { "✓" } else { "✗" };
        println!("  {icon} [{}] {}: {}", i + 1, check.label, check.detail);
        if !check.passed {
            all_pass = false;
        }
    }
    println!();

    if all_pass {
        println!("All checks passed.");
        Ok(())
    } else {
        let failed: usize = checks.iter().filter(|c| !c.passed).count();
        println!("{failed} check(s) failed. Address the issues above and re-run `corelink doctor`.");
        // Return an error so the process exits with code 1.
        anyhow::bail!("{failed} check(s) failed");
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Resolve effective (endpoint, token) without panicking on missing config.
fn resolve_endpoint_token() -> (String, String) {
    match Config::load() {
        Ok(cfg) => (cfg.endpoint, cfg.token),
        Err(_) => {
            // Config file absent — try env vars, fall back to defaults.
            let ep = std::env::var("CORELINK_ENDPOINT")
                .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
            let tok = std::env::var("CORELINK_TOKEN").unwrap_or_default();
            (ep, tok)
        }
    }
}

fn is_valid_url(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://")
}

/// Redact a token/secret — show only the first `n` chars + "...".
fn redact(s: &str, n: usize) -> String {
    let show: String = s.chars().take(n).collect();
    format!("{show}[REDACTED]")
}

/// Check whether a binary is on PATH (by running `which`/`where`).
fn bin_on_path(bin: &str) -> bool {
    let check_cmd = if cfg!(windows) { "where" } else { "which" };
    Command::new(check_cmd)
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Read a file and check whether it contains `needle`.
fn file_contains(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains(needle))
        .unwrap_or(false)
}

/// Probe the endpoint via POST /v1/ping.
///
/// Returns `(reach_ok, reach_detail, auth_ok, auth_detail, elapsed_ms)`.
fn probe_endpoint(
    url: &str,
    token: &str,
    verbose: bool,
) -> (bool, String, bool, String, u128) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                false,
                format!("failed to build HTTP client: {e}"),
                false,
                "skipped (client build failed)".to_string(),
                0,
            );
        }
    };

    let started = Instant::now();
    let result = client
        .post(url)
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .body("")
        .send();
    let elapsed_ms = started.elapsed().as_millis();

    match result {
        Err(e) => {
            let detail = if e.is_timeout() {
                format!("timed out after 5 s reaching {url} — check network/firewall")
            } else if e.is_connect() {
                format!(
                    "connection refused to {url} — verify endpoint or check \
                     `corelink config show`"
                )
            } else {
                format!("network error: {e}")
            };
            (false, detail, false, "skipped (unreachable)".to_string(), elapsed_ms)
        }
        Ok(resp) => {
            let status = resp.status();
            if verbose {
                println!("  [verbose] POST {url} → HTTP {status} ({elapsed_ms} ms)");
            }
            let reach_ok = true;
            let reach_detail = String::new();

            let (auth_ok, auth_detail) = if status.is_success() {
                (true, String::new())
            } else if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                (
                    false,
                    format!(
                        "HTTP {status} — token rejected; run `corelink config show` to \
                         verify token, or issue a new PAT at \
                         https://corelink-app.humangr.com"
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "HTTP {status} — unexpected response from {url}; \
                         check endpoint configuration"
                    ),
                )
            };

            (reach_ok, reach_detail, auth_ok, auth_detail, elapsed_ms)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── is_valid_url ─────────────────────────────────────────────────────────

    #[test]
    fn valid_url_https() {
        assert!(is_valid_url("https://corelink-api.humangr.com"));
    }

    #[test]
    fn valid_url_http_local() {
        assert!(is_valid_url("http://localhost:8080"));
    }

    #[test]
    fn invalid_url_bare_hostname() {
        assert!(!is_valid_url("corelink-api.humangr.com"));
    }

    // ── redact ───────────────────────────────────────────────────────────────

    #[test]
    fn redact_shows_prefix_only() {
        let s = redact("ct_supersecrettoken", 6);
        assert!(s.starts_with("ct_sup"));
        assert!(s.contains("[REDACTED]"));
        assert!(!s.contains("secrettoken"));
    }

    // ── bin_on_path (smoke) ───────────────────────────────────────────────────

    #[test]
    fn bin_on_path_finds_sh() {
        // `sh` is virtually universal on Unix-like systems used in CI.
        #[cfg(unix)]
        assert!(bin_on_path("sh"), "sh must be on PATH");
    }

    #[test]
    fn bin_on_path_returns_false_for_nonexistent() {
        assert!(!bin_on_path("__corelink_totally_nonexistent_binary_xyz__"));
    }

    // ── file_contains ─────────────────────────────────────────────────────────

    #[test]
    fn file_contains_returns_true_when_needle_present() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("test.txt");
        std::fs::write(&f, "# corelink-managed (do not edit between markers)\n").unwrap();
        assert!(file_contains(&f, "corelink-managed"));
    }

    #[test]
    fn file_contains_returns_false_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("test.txt");
        std::fs::write(&f, "build --jobs=4\n").unwrap();
        assert!(!file_contains(&f, "corelink-managed"));
    }

    #[test]
    fn file_contains_returns_false_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("nonexistent.txt");
        assert!(!file_contains(&f, "anything"));
    }
}
