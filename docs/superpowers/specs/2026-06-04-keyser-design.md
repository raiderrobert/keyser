# Keyser — Design Spec

> "The greatest trick your secrets ever played was convincing the world they didn't exist."

A Rust CLI tool that stores environment variables in macOS Keychain and injects them into subprocesses at runtime. A modern, maintained port of [envchain](https://github.com/sorah/envchain) using Apple's current `SecItem` APIs.

## Goals

- **1:1 CLI compatibility with envchain** — same flags, same behavior, drop-in replacement (minus data migration)
- **Modern Keychain APIs** — use `SecItem*` exclusively, avoiding the deprecated `SecKeychain*` APIs that envchain relies on
- **Own Keychain namespace** — `keyser-` prefix instead of `envchain-`, clean separation
- **Open source** — MIT licensed, published at `raiderrobert/keyser`
- **macOS only** — no Linux backend in v1. Direct `security-framework` crate bindings, no cross-platform abstraction layer

## Non-Goals

- Cross-platform support (Linux, Windows)
- GUI / Keychain Access.app integration beyond standard item visibility
- Importing/migrating envchain data
- Subcommand-style CLI (that's a future improvement)

## CLI Interface

```
keyser --set [-n] [-p|-P] NAMESPACE ENV [ENV..]
keyser NAMESPACE[,NS2] CMD [ARGS..]
keyser --list [NAMESPACE] [-v]
keyser --unset NAMESPACE ENV [ENV..]
```

### Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--set` | `-s` | Store mode: prompt for values and save to Keychain |
| `--noecho` | `-n` | Hide input when prompting (secure input) |
| `--require-passphrase` | `-p` | Set ACL to always require Keychain password on access |
| `--no-require-passphrase` | `-P` | Set ACL to allow access without re-prompting |
| `--list` | `-l` | List mode: show namespaces, or keys within a namespace |
| `--show-value` | `-v` | Show values when listing keys (requires a namespace) |
| `--unset` | | Delete specific keys from a namespace |

### Exec Mode

When the first argument is not a flag, keyser treats it as a namespace (or comma-separated namespaces), looks up all env vars for those namespaces, sets them in the environment, and execs the remaining arguments.

```
keyser aws terraform plan
keyser aws,github gh pr create
```

If a namespace doesn't exist, keyser prints a warning to stderr and continues (matching envchain behavior).

## Keychain Data Model

Each env var is stored as a **generic password** item in the user's default keychain:

| Attribute | Value | Keychain Constant |
|-----------|-------|-------------------|
| **Service** | `keyser-<NAMESPACE>` | `kSecAttrService` |
| **Account** | env var key (e.g. `AWS_SECRET_ACCESS_KEY`) | `kSecAttrAccount` |
| **Password** | env var value | `kSecValueData` |
| **Description** | `"keyser"` | `kSecAttrDescription` |
| **Label** | `keyser-<NAMESPACE>-<KEY>` | `kSecAttrLabel` |

### Query Patterns

- **List all namespaces:** Search for all generic passwords where description = `"keyser"`, extract unique namespace names from the service attribute by stripping the `keyser-` prefix.
- **List keys in a namespace:** Search for all generic passwords where service = `keyser-<NAMESPACE>`.
- **Get a single value:** Search where service = `keyser-<NAMESPACE>` and account = `<KEY>`, load data.
- **Save a value:** Use `ItemAddOptions` with service, account, description, label. If the item already exists, use `update_item` to modify it.
- **Delete a value:** Use `ItemSearchOptions` to find the item, then `delete()`.

## Project Structure

```
keyser/
├── .github/workflows/
│   ├── ci.yml                    # fmt + clippy + test (macos-latest runner)
│   ├── pr-title.yml              # conventional PR title enforcement
│   ├── release-please.yml        # auto changelog + version bump
│   └── release.yml               # build macOS binary, upload to GH release
├── release-please-config.json
├── .release-please-manifest.json
├── justfile
├── install.sh
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs                   # CLI parsing, entry point, exec logic
│   └── keychain.rs               # all Keychain operations
```

### `src/main.rs`

Responsibilities:
- Parse CLI arguments (hand-rolled, no clap)
- Dispatch to set/list/unset/exec based on flags
- Handle `--set` interactive prompting (using `rpassword` for `--noecho`)
- In exec mode: call `keychain::search_values()`, `setenv` each result, then `execvp` the command

### `src/keychain.rs`

Public API (mirrors envchain's C interface):

```rust
pub fn search_namespaces() -> Result<Vec<String>>
pub fn search_values(namespace: &str) -> Result<Vec<(String, String)>>
pub fn save_value(namespace: &str, key: &str, value: &str, require_passphrase: Option<bool>) -> Result<()>
pub fn delete_value(namespace: &str, key: &str) -> Result<()>
```

All functions use `security-framework`'s `ItemSearchOptions`, `ItemAddOptions`, and related types. Errors are mapped to a simple `KeyserError` enum wrapping `security_framework::base::Error`.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `security-framework` | macOS Keychain access via `SecItem*` APIs |
| `core-foundation` | `CFDictionary`/`CFString` types used by `security-framework` |
| `rpassword` | Secure password input for `--noecho` mode |

No `clap` — envchain's flag parsing is simple enough to do by hand, and matching the exact `--set NAMESPACE ENV` positional style is easier without a framework.

## CI/CD

Follows the pattern from `raiderrobert/diecut` and `raiderrobert/graft`:

### `ci.yml`
- Trigger: push to main or PR, filtered to `src/**`, `Cargo.*`, `justfile`, workflow file
- Runner: **`macos-latest`** (not ubuntu — `security-framework` requires macOS)
- Steps: checkout, rust-toolchain, setup-just, rust-cache, `just check`

### `pr-title.yml`
- Validates conventional commit format on PR titles

### `release-please.yml`
- Runs on push to main
- Creates/updates a release PR with changelog
- On release, dispatches `release.yml` with the tag

### `release.yml`
- Triggered by `workflow_dispatch` with a tag input
- Build matrix: `aarch64-apple-darwin` on `macos-latest` (macOS-only, no Linux target)
- Packages binary as `keyser-aarch64-macos.tar.gz`
- Uploads to GitHub Release

### `justfile`

```just
default:
    @just --list

check:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test

test *args:
    cargo test {{args}}

build:
    cargo build --release

fmt:
    cargo fmt

install:
    cargo install --path .
```

### `install.sh`

Curl-pipe-sh installer, macOS/aarch64 only:
```
curl -fsSL https://raw.githubusercontent.com/raiderrobert/keyser/main/install.sh | sh
```

## Error Handling

- Keychain errors from `security-framework` are wrapped in a `KeyserError` enum
- `errSecItemNotFound` is handled gracefully:
  - In exec mode: warn to stderr, continue execution (matches envchain)
  - In `--unset`: silent no-op (item already doesn't exist)
  - In `--set`: treated as "create new" (not an error)
- All other Keychain errors print a human-readable message via `SecCopyErrorMessageString` and exit with code 10 (matching envchain's convention)

## Testing

Unit testing Keychain operations is tricky — they require a real Keychain. Strategy:

- **Integration tests** that create/read/delete items in a test namespace (`keyser-test-*`)
- Tests clean up after themselves by deleting all items they create
- CI runs on macOS so Keychain access works
- No mocking — real Keychain operations are the point

## Lessons from envchain Issues

These are known bugs and feature requests from envchain's issue tracker that inform our implementation:

1. **Duplicate namespaces in `--list`** (envchain #24, #34): envchain deduplicates by sorting and comparing adjacent names, which is fragile. Keyser uses a `BTreeSet<String>` for namespace collection — duplicates are impossible.

2. **Multiline values** (envchain #20, #33): envchain's `readline()` only reads one line, so piping multiline secrets (SSH keys, .p8 certs) silently truncates. Keyser handles this: when stdin is not a TTY (i.e., piped input), read to EOF. When stdin is a TTY, prompt interactively per line (matching envchain's current behavior).

3. **Scripted/headless `--set`** (envchain #15, #35): Users want `echo "val" | keyser --set ns KEY`. Keyser detects TTY vs pipe on stdin and behaves accordingly — no `readline` dependency means this works naturally.

4. **`-p` and Touch ID** (envchain #28, #39): envchain uses deprecated `SecACL` APIs for `--require-passphrase`. The modern equivalent is `SecAccessControlCreateWithFlags(..., kSecAccessControlUserPresence, ...)`, which enables **both** password and Touch ID. This is a free UX upgrade. The `security-framework` crate exposes `SecAccessControl` with `ProtectionMode`, which should cover this.

5. **iCloud sync prevention** (envchain #38): We must explicitly set `kSecAttrSynchronizable` to `false` when adding items to prevent accidental iCloud Keychain sync.

## Known Risks

- **`-p`/`-P` ACL granularity:** The `security-framework` crate's `SecAccessControl` may not expose all the options needed for the passphrase requirement behavior. If it doesn't, we either drop to `security-framework-sys` FFI for this one feature, or defer `-p`/`-P` to a follow-up. Core functionality (set/get/list/unset/exec) works without it.

## Future Improvements (Not in v1)

- Subcommand-style CLI (`keyser set`, `keyser run`, etc.)
- `--version` flag
- Shell completions
- Import from envchain (`keyser import-envchain`)
- Linux backend via `libsecret` / D-Bus Secret Service
- Homebrew formula
