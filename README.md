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
token = "ct_xxx..."
region = "auto"                                  # optional
endpoint = "https://corelink-api.humangr.com"    # optional
```

## Subcommands

| Command                   | Description                                                                         |
| ------------------------- | ----------------------------------------------------------------------------------- |
| `corelink ping`           | Health-check the endpoint (POSTs to `/v1/ping`). Supports `--endpoint` / `--token`. |
| `corelink bazel-init`     | Idempotently inject a remote-cache stanza into `.bazelrc`.                          |
| `corelink cargo-init`     | Inject `.cargo/config.toml` sccache stanza (project-local).                        |
| `corelink npm-init`       | Inject `turbo.json` remoteCache stanza (Turborepo).                                 |
| `corelink docker-init`    | Configure Docker BuildKit remote registry cache.                                    |
| `corelink doctor`         | 9-point environment diagnostic (auth, endpoint, installed tools).                   |
| `corelink config show`    | Print resolved config (token redacted).                                             |

---

### `corelink bazel-init`

Run inside any Bazel workspace (with `WORKSPACE` or `MODULE.bazel`). Adds a marker block to `.bazelrc`:

```bazelrc
# corelink-managed (do not edit between markers)
build --remote_cache=https://corelink-api.humangr.com
build --remote_header=authorization=Bearer\ ct_xxx
build --remote_upload_local_results=true
# /corelink-managed
```

Re-running is a no-op when the block is already up-to-date.

---

### `corelink cargo-init`

Writes a project-local `.cargo/config.toml` configuring [sccache](https://github.com/mozilla/sccache)
as the `rustc-wrapper` pointing at CoreLink's WebDAV cache endpoint.

> **Requires `sccache` on PATH.**
> Install via `cargo install sccache --locked` or `brew install sccache`.

```sh
corelink cargo-init [--endpoint <URL>] [--token <PAT>]
```

Creates or updates `.cargo/config.toml`:

```toml
# corelink-managed (do not edit between markers)
[build]
rustc-wrapper = "sccache"

[env]
SCCACHE_ENDPOINT = "https://corelink-api.humangr.com/cargo"
SCCACHE_BUCKET = "corelink"
SCCACHE_AUTH_TYPE = "bearer"
SCCACHE_AUTH_TOKEN = "ct_xxx"
# /corelink-managed
```

Re-running is a no-op when the block is already up-to-date. Does **not** modify
`~/.cargo/config.toml` (only the project-local `.cargo/config.toml`).

---

### `corelink npm-init`

Configures [Turborepo](https://turbo.build/) remote caching in `turbo.json`.
Creates `turbo.json` if absent; merges the `remoteCache` key if it already exists.

```sh
corelink npm-init [--endpoint <URL>] [--token <PAT>]
```

Merges into `turbo.json`:

```json
{
  "remoteCache": {
    "signature": true,
    "enabled": true,
    "apiUrl": "https://corelink-api.humangr.com",
    "token": "ct_xxx",
    "teamId": "corelink"
  }
}
```

Re-running is a no-op when `remoteCache.apiUrl` already matches the endpoint.

---

### `corelink docker-init`

Configures Docker BuildKit to use CoreLink's OCI registry cache layer.
Writes `.docker/buildx-cache.json` and appends it to `.gitignore` (contains a token).

```sh
corelink docker-init [--endpoint <URL>] [--token <PAT>]
```

Then use the generated cache flags in your Docker builds:

```sh
docker build \
  --cache-from type=registry,ref=https://corelink-api.humangr.com/cache \
  --cache-to   type=registry,ref=https://corelink-api.humangr.com/cache,mode=max \
  .
```

Re-running is a no-op when `.docker/buildx-cache.json` already points at the same endpoint.

---

### `corelink doctor`

Runs a 9-point checklist to verify your environment is correctly configured:

```sh
corelink doctor [--verbose]
```

```
corelink doctor — environment diagnostic

  ✓ [1] CLI version: 0.1.0 (aarch64)
  ✓ [2] endpoint: https://corelink-api.humangr.com (from ~/.corelink/config.toml)
  ✓ [3] token: ct_abc1[REDACTED]... (~/.corelink/config.toml)
  ✓ [4] endpoint reachable: POST https://corelink-api.humangr.com/v1/ping (42 ms)
  ✓ [5] auth valid: HTTP 200 (42 ms)
  ✓ [6] bazel (bazel-init not applied): skipped — no .bazelrc corelink block detected
  ✓ [7] sccache (cargo-init applied): sccache found on PATH
  ✓ [8] turbo (npm-init not applied): skipped — no turbo.json remoteCache block detected
  ✓ [9] docker buildx (docker-init not applied): skipped — no .docker/buildx-cache.json detected

All checks passed.
```

Exits 0 if all checks pass, 1 if any fail. `--verbose` prints HTTP bodies and full
environment variable values (token always redacted).

---

## Status

> **Note** — `https://corelink-api.humangr.com/v1/ping` goes live alongside Wave 32
> production deploy. Until then, point `endpoint` in `~/.corelink/config.toml` at
> your dev/staging URL.

## License

Apache-2.0 — see [LICENSE](./LICENSE).
