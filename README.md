# corelink-cli

Pre-built binaries for the **CoreLink CLI** — the client for [CoreLink](https://corelink.humangr.com), a content-addressable cache backing Bazel, Buck2, Cargo, Docker, and ML pipelines.

> **Source lives in [`humangr-labs/corelink-server`](https://github.com/humangr-labs/corelink-server)** (`tools/cli`). This repo is a **download mirror** for the install one-liner — releases publish binaries here so the public `curl | sh` install works without GitHub auth.

## Install

```sh
curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=<your PAT>
```

The one-liner detects your OS + arch, downloads the matching binary from the [latest release](https://github.com/humangr-labs/corelink-cli/releases/latest), writes `~/.corelink/config.toml`, and verifies connectivity to `https://corelink-api.humangr.com`.

## Manual download

```
https://github.com/humangr-labs/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}
```

Where `OS` ∈ `linux`, `darwin`; `ARCH` ∈ `x86_64`, `aarch64`. Windows binary deferred to v0.1.1.

## Usage

```sh
corelink whoami                      # verify auth + show tenant
corelink put <file>                  # upload bytes to CAS
corelink get <sha256>                # download by hash
corelink ac put <digest> <result>    # action cache write
corelink ac get <digest>             # action cache read
```

PAT is read from `$CORELINK_PAT` env var, then `~/.corelink/config.toml`.

## Verify a release

```sh
curl -fsSL -O https://github.com/humangr-labs/corelink-cli/releases/latest/download/checksums.txt
shasum -a 256 -c checksums.txt
```

## License

Apache-2.0 — see [`LICENSE`](./LICENSE).

## Issues + source

File bugs at [humangr-labs/corelink-server/issues](https://github.com/humangr-labs/corelink-server/issues).
