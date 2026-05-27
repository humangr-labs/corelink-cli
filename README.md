# corelink

CoreLink CLI — client for the HuGR shared content-addressable cache (Bazel / Buck2 / Cargo / Docker / ML).

## Install

```sh
# One-liner (recommended)
curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=<PAT>

# Or via cargo
cargo install corelink

# Or grab a pre-built binary
# https://github.com/humangr-labs/corelink-cli/releases/latest
```

## Configure

Create `~/.corelink/config.toml`:

```toml
token = "ck_xxx..."
region = "auto"                                  # optional
endpoint = "https://corelink-api.humangr.com"    # optional
```

## Subcommands

| Command              | Description                                                          |
| -------------------- | -------------------------------------------------------------------- |
| `corelink ping`      | Health-check the endpoint (POSTs to `/v1/ping`). Supports `--endpoint` / `--token` overrides. |
| `corelink bazel-init`| Idempotently inject a remote-cache stanza into `.bazelrc`.           |
| `corelink config show` | Print resolved config (token redacted).                            |

### `corelink bazel-init`

Run inside any Bazel workspace (with `WORKSPACE` or `MODULE.bazel`). Adds a marker block to `.bazelrc`:

```bazelrc
# corelink-managed (do not edit between markers)
build --remote_cache=https://corelink-api.humangr.com
build --remote_header=authorization=Bearer\ ck_xxx
build --remote_upload_local_results=true
# /corelink-managed
```

Re-running is a no-op when the block is up-to-date.

## Status

> **TODO** — `https://corelink-api.humangr.com/v1/ping` currently returns 404. The
> control-plane endpoint will go live alongside Wave 32 production deploy. Until then,
> point `endpoint` in `~/.corelink/config.toml` at your dev/staging URL.

## License

Apache-2.0 — see [LICENSE](./LICENSE).
