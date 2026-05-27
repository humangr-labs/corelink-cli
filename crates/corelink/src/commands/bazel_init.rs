//! `corelink bazel-init` — idempotently inject a remote-cache stanza into `.bazelrc`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;

const MARKER_BEGIN: &str = "# corelink-managed (do not edit between markers)";
const MARKER_END: &str = "# /corelink-managed";

/// Run the `bazel-init` subcommand.
pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("cwd unreadable")?;
    if !has_bazel_workspace(&cwd) {
        anyhow::bail!(
            "no WORKSPACE / WORKSPACE.bazel / MODULE.bazel found in {}",
            cwd.display()
        );
    }

    let cfg = Config::load().context("failed to load ~/.corelink/config.toml")?;
    let bazelrc = cwd.join(".bazelrc");
    let existing = match fs::read_to_string(&bazelrc) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("failed to read .bazelrc"),
    };

    let stanza = render_stanza(&cfg.endpoint, &cfg.token);
    let updated = upsert_block(&existing, &stanza);
    if updated == existing {
        println!("corelink: .bazelrc already up-to-date (no-op)");
    } else {
        fs::write(&bazelrc, updated).context("failed to write .bazelrc")?;
        println!("corelink: updated {}", bazelrc.display());
    }
    Ok(())
}

fn has_bazel_workspace(dir: &Path) -> bool {
    ["WORKSPACE", "WORKSPACE.bazel", "MODULE.bazel"]
        .iter()
        .any(|f| dir.join(f).exists())
}

fn render_stanza(endpoint: &str, token: &str) -> String {
    format!(
        "{MARKER_BEGIN}\nbuild --remote_cache={endpoint}\nbuild --remote_header=authorization=Bearer\\ {token}\nbuild --remote_upload_local_results=true\n{MARKER_END}\n"
    )
}

/// Replace the marker block if present, else append it.
fn upsert_block(existing: &str, new_block: &str) -> String {
    if let (Some(begin), Some(end)) = (existing.find(MARKER_BEGIN), existing.find(MARKER_END)) {
        if end > begin {
            // Include trailing newline after MARKER_END if present.
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

    #[test]
    fn upsert_appends_when_absent() {
        let existing = "build --jobs=4\n";
        let block = render_stanza("https://x", "tok");
        let out = upsert_block(existing, &block);
        assert!(out.starts_with("build --jobs=4\n"));
        assert!(out.contains("corelink-managed"));
        assert!(out.contains("https://x"));
    }

    #[test]
    fn upsert_is_idempotent() {
        let block = render_stanza("https://x", "tok");
        let once = upsert_block("", &block);
        let twice = upsert_block(&once, &block);
        assert_eq!(once, twice);
    }

    #[test]
    fn upsert_replaces_existing_block() {
        let old = render_stanza("https://OLD", "oldtok");
        let with_old = upsert_block("build --jobs=4\n", &old);
        let new = render_stanza("https://NEW", "newtok");
        let with_new = upsert_block(&with_old, &new);
        assert!(with_new.contains("https://NEW"));
        assert!(!with_new.contains("https://OLD"));
        assert!(with_new.contains("build --jobs=4"));
    }
}
