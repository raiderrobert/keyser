# keyser

> "The greatest trick your secrets ever played was convincing the world they didn't exist."

A Rust CLI that stores environment variables in macOS Keychain and injects them into subprocesses at runtime. A modern, maintained port of [envchain](https://github.com/sorah/envchain).

## Why?

Putting secrets in `.bashrc`, `.env` files, or shell history is a security risk. `keyser` stores them in macOS Keychain (encrypted, protected by your login password or Touch ID) and injects them only when you ask.

## Install

```sh
# From source
cargo install --path .

# Or via the install script
curl -fsSL https://raw.githubusercontent.com/raiderrobert/keyser/main/install.sh | sh
```

## Usage

### Save secrets

```sh
keyser --set aws AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
# Prompts for each value interactively
```

Pipe values for scripted use:

```sh
echo "my-secret-key" | keyser --set aws AWS_SECRET_ACCESS_KEY
```

Use `--noecho` to hide input:

```sh
keyser --set --noecho aws AWS_SECRET_ACCESS_KEY
```

### Run commands with secrets

```sh
keyser aws terraform plan
keyser aws s3 ls
```

Multiple namespaces at once:

```sh
keyser aws,github gh pr create
```

### List

```sh
# List all namespaces
keyser --list

# List keys in a namespace
keyser --list aws

# Show values too
keyser --list aws -v
```

### Remove

```sh
keyser --unset aws AWS_ACCESS_KEY_ID
```

## How it works

Each secret is stored as a generic password in your macOS login Keychain:

- **Service:** `keyser-<namespace>`
- **Account:** the environment variable name
- **Password:** the value

Items are visible in Keychain Access.app and protected by your login password. iCloud sync is explicitly disabled.

## Differences from envchain

- Written in Rust (envchain is C)
- Uses modern `SecItem*` APIs (envchain uses deprecated `SecKeychain*` APIs)
- Own Keychain namespace (`keyser-` prefix, not `envchain-`)
- Handles piped/multiline input correctly
- No duplicate namespaces in `--list`

## License

MIT
