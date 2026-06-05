# Keyser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI that stores environment variables in macOS Keychain and injects them into subprocesses — a modern port of envchain.

**Architecture:** Single binary with two source modules: `main.rs` for CLI parsing/dispatch and `keychain.rs` for all macOS Keychain operations via `security-framework`. Keychain items use a `keyser-` service prefix with `kSecAttrDescription = "keyser"` for namespace discovery. We build our own `CFDictionary` for `SecItemAdd` to set `kSecAttrSynchronizable = false` (not exposed by `ItemAddOptions`).

**Tech Stack:** Rust, `security-framework` 3.7, `security-framework-sys`, `core-foundation`, `rpassword` 7.5

---

## File Map

| File | Responsibility |
|------|----------------|
| `Cargo.toml` | Package metadata, dependencies |
| `src/main.rs` | CLI arg parsing, interactive prompting, exec dispatch |
| `src/keychain.rs` | All Keychain CRUD: save, search, delete, namespace listing |
| `tests/integration_test.rs` | End-to-end tests against real Keychain using test namespaces |
| `justfile` | Dev commands: check, test, build, fmt, install |
| `.github/workflows/ci.yml` | CI: fmt + clippy + test on macos-latest |
| `.github/workflows/pr-title.yml` | Conventional PR title enforcement |
| `.github/workflows/release-please.yml` | Auto changelog + version bump |
| `.github/workflows/release.yml` | Build macOS binary, upload to GH release |
| `release-please-config.json` | Release-please configuration |
| `.release-please-manifest.json` | Version manifest |
| `install.sh` | Curl-pipe-sh installer |
| `.gitignore` | Standard Rust + tooling ignores |

---

### Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/keychain.rs`
- Create: `.gitignore`

- [ ] **Step 1: Initialize the Cargo project**

Run:
```bash
cd ~/repos/keyser && cargo init --name keyser
```

- [ ] **Step 2: Replace Cargo.toml with full config**

Replace `Cargo.toml` with:

```toml
[package]
name = "keyser"
version = "0.1.0"
edition = "2021"
license = "MIT"
rust-version = "1.75"
description = "Environment variables meet macOS Keychain — a modern envchain port"
repository = "https://github.com/raiderrobert/keyser"

[[bin]]
name = "keyser"
path = "src/main.rs"

[dependencies]
security-framework = "3.7"
security-framework-sys = "3"
core-foundation = "0.10"
rpassword = "7.5"
```

- [ ] **Step 3: Write .gitignore**

Replace `.gitignore` with:

```gitignore
# Rust
/target
*.profraw
*.profdata

# Tooling
.worktrees/
.beads/
.claude/
CLAUDE.md

# macOS
.DS_Store

# IDEs
.vscode/
.idea/
```

- [ ] **Step 4: Create stub source files**

Write `src/keychain.rs`:

```rust
use std::fmt;

#[derive(Debug)]
pub enum KeyserError {
    Keychain(security_framework::base::Error),
    ItemNotFound,
}

impl fmt::Display for KeyserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyserError::Keychain(e) => write!(f, "Keychain error: {e}"),
            KeyserError::ItemNotFound => write!(f, "Item not found"),
        }
    }
}

impl std::error::Error for KeyserError {}

impl From<security_framework::base::Error> for KeyserError {
    fn from(e: security_framework::base::Error) -> Self {
        KeyserError::Keychain(e)
    }
}

pub type Result<T> = std::result::Result<T, KeyserError>;
```

Write `src/main.rs`:

```rust
mod keychain;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_help(&args[0]);
        std::process::exit(2);
    }
}

fn print_help(name: &str) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        "{name} version {version}\n\n\
         Usage:\n  \
           Add variables\n    \
             {name} (--set|-s) [--[no-]require-passphrase|-p|-P] [--noecho|-n] NAMESPACE ENV [ENV ..]\n  \
           Execute with variables\n    \
             {name} NAMESPACE CMD [ARG ...]\n  \
           List namespaces\n    \
             {name} --list\n  \
           Remove variables\n    \
             {name} --unset NAMESPACE ENV [ENV ..]\n\n\
         Options:\n  \
           --set (-s):       Add keychain item of environment variable ENV for namespace NAMESPACE.\n  \
           --noecho (-n):    Enable noecho mode when prompting values.\n  \
           --require-passphrase (-p), --no-require-passphrase (-P):\n                    \
             Replace the item's ACL list to require passphrase (or not)."
    );
}
```

- [ ] **Step 5: Verify it compiles**

Run:
```bash
cd ~/repos/keyser && cargo build
```

Expected: Compiles with no errors. Warnings about unused imports are fine at this stage.

- [ ] **Step 6: Commit**

```bash
cd ~/repos/keyser
git add Cargo.toml Cargo.lock src/main.rs src/keychain.rs .gitignore
git commit -m "feat: project scaffolding with dependencies and stubs"
```

---

### Task 2: Keychain Save and Search

**Files:**
- Modify: `src/keychain.rs`
- Create: `tests/integration_test.rs`

- [ ] **Step 1: Write failing integration test for save + search**

Create `tests/integration_test.rs`:

```rust
use std::process::Command;

const TEST_NS: &str = "keyser-integration-test";

fn keyser_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_keyser"))
}

fn cleanup_test_namespace() {
    // Use the keyser binary itself to unset, ignore errors if items don't exist
    let output = keyser_bin()
        .args(["--list", TEST_NS])
        .output()
        .expect("failed to run keyser");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for key in stdout.lines() {
        let key = key.trim();
        if !key.is_empty() {
            keyser_bin()
                .args(["--unset", TEST_NS, key])
                .output()
                .ok();
        }
    }
}

#[test]
fn test_set_and_list_keys() {
    cleanup_test_namespace();

    // Set a value by piping stdin
    let status = keyser_bin()
        .args(["--set", TEST_NS, "TEST_VAR_ONE"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"secret_value_one\n")?;
            child.wait()
        })
        .expect("failed to run keyser --set");
    assert!(status.success(), "keyser --set failed");

    // List keys in the namespace
    let output = keyser_bin()
        .args(["--list", TEST_NS])
        .output()
        .expect("failed to run keyser --list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TEST_VAR_ONE"), "expected TEST_VAR_ONE in list output, got: {stdout}");

    // List with --show-value
    let output = keyser_bin()
        .args(["--list", TEST_NS, "-v"])
        .output()
        .expect("failed to run keyser --list -v");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TEST_VAR_ONE=secret_value_one"), "expected key=value in output, got: {stdout}");

    cleanup_test_namespace();
}

#[test]
fn test_set_and_exec() {
    cleanup_test_namespace();

    // Set a value
    let status = keyser_bin()
        .args(["--set", TEST_NS, "MY_SECRET"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"hunter2\n")?;
            child.wait()
        })
        .expect("failed to run keyser --set");
    assert!(status.success());

    // Exec env and check the variable is injected
    let output = keyser_bin()
        .args([TEST_NS, "env"])
        .output()
        .expect("failed to run keyser exec");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MY_SECRET=hunter2"), "expected MY_SECRET=hunter2 in env output, got: {stdout}");

    cleanup_test_namespace();
}

#[test]
fn test_unset() {
    cleanup_test_namespace();

    // Set then unset
    let status = keyser_bin()
        .args(["--set", TEST_NS, "REMOVEME"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"gone\n")?;
            child.wait()
        })
        .expect("failed to run keyser --set");
    assert!(status.success());

    let status = keyser_bin()
        .args(["--unset", TEST_NS, "REMOVEME"])
        .status()
        .expect("failed to run keyser --unset");
    assert!(status.success());

    // Verify it's gone
    let output = keyser_bin()
        .args(["--list", TEST_NS])
        .output()
        .expect("failed to run keyser --list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("REMOVEME"), "REMOVEME should be gone, got: {stdout}");

    cleanup_test_namespace();
}

#[test]
fn test_list_namespaces() {
    cleanup_test_namespace();

    // Set a value so the namespace exists
    let status = keyser_bin()
        .args(["--set", TEST_NS, "NS_CHECK"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"val\n")?;
            child.wait()
        })
        .expect("failed to run keyser --set");
    assert!(status.success());

    // List all namespaces (no namespace arg)
    let output = keyser_bin()
        .args(["--list"])
        .output()
        .expect("failed to run keyser --list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Strip the keyser- prefix that search_namespaces returns
    assert!(stdout.contains(TEST_NS), "expected {TEST_NS} in namespace list, got: {stdout}");

    // Verify no duplicates
    let count = stdout.lines().filter(|l| l.trim() == TEST_NS).count();
    assert_eq!(count, 1, "namespace appeared {count} times, expected 1");

    cleanup_test_namespace();
}

#[test]
fn test_exec_multiple_namespaces() {
    let ns1 = "keyser-integration-test-multi1";
    let ns2 = "keyser-integration-test-multi2";

    // Clean up both
    for ns in [ns1, ns2] {
        let output = keyser_bin().args(["--list", ns]).output().ok();
        if let Some(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for key in stdout.lines() {
                let key = key.trim();
                if !key.is_empty() {
                    keyser_bin().args(["--unset", ns, key]).output().ok();
                }
            }
        }
    }

    // Set values in two namespaces
    for (ns, key, val) in [(ns1, "VAR_A", "aaa"), (ns2, "VAR_B", "bbb")] {
        let status = keyser_bin()
            .args(["--set", ns, key])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(format!("{val}\n").as_bytes())?;
                child.wait()
            })
            .expect("failed to set");
        assert!(status.success());
    }

    // Exec with comma-separated namespaces
    let combined = format!("{ns1},{ns2}");
    let output = keyser_bin()
        .args([&combined, "env"])
        .output()
        .expect("failed to exec");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("VAR_A=aaa"), "missing VAR_A in: {stdout}");
    assert!(stdout.contains("VAR_B=bbb"), "missing VAR_B in: {stdout}");

    // Clean up
    for (ns, key) in [(ns1, "VAR_A"), (ns2, "VAR_B")] {
        keyser_bin().args(["--unset", ns, key]).output().ok();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```bash
cd ~/repos/keyser && cargo test -- --test-threads=1
```

Expected: Tests fail because `--set`, `--list`, `--unset` are not implemented yet.

- [ ] **Step 3: Implement keychain.rs — save_value**

Replace `src/keychain.rs` with the full implementation:

```rust
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFMutableDictionary;
use core_foundation::string::CFString;
use security_framework::item::{ItemClass, ItemSearchOptions, ItemUpdateValue, ItemUpdateOptions, Limit, SearchResult};
use security_framework_sys::base::{errSecItemNotFound, SecItemAdd};
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrDescription, kSecAttrLabel, kSecAttrService,
    kSecAttrSynchronizable, kSecClass, kSecClassGenericPassword, kSecValueData,
};
use std::collections::BTreeSet;
use std::fmt;

const SERVICE_PREFIX: &str = "keyser-";
const ITEM_DESCRIPTION: &str = "keyser";

#[derive(Debug)]
pub enum KeyserError {
    Keychain(security_framework::base::Error),
    ItemNotFound,
}

impl fmt::Display for KeyserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyserError::Keychain(e) => write!(f, "Keychain error: {e}"),
            KeyserError::ItemNotFound => write!(f, "Item not found"),
        }
    }
}

impl std::error::Error for KeyserError {}

impl From<security_framework::base::Error> for KeyserError {
    fn from(e: security_framework::base::Error) -> Self {
        if e.code() == errSecItemNotFound {
            KeyserError::ItemNotFound
        } else {
            KeyserError::Keychain(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, KeyserError>;

fn service_name(namespace: &str) -> String {
    format!("{SERVICE_PREFIX}{namespace}")
}

fn label_name(namespace: &str, key: &str) -> String {
    format!("{SERVICE_PREFIX}{namespace}-{key}")
}

pub fn save_value(
    namespace: &str,
    key: &str,
    value: &str,
    _require_passphrase: Option<bool>,
) -> Result<()> {
    let service = service_name(namespace);
    let label = label_name(namespace, key);

    // Try to update existing item first
    match find_item(namespace, key) {
        Ok(_) => {
            let mut update = ItemUpdateOptions::default();
            update.value = Some(ItemUpdateValue::Data {
                class: ItemClass::generic_password(),
                data: CFData::from_buffer(value.as_bytes()),
            });
            update.label = Some(CFString::new(&label));
            let search = ItemSearchOptions::new()
                .class(ItemClass::generic_password())
                .service(&service)
                .account(key)
                .to_owned();
            security_framework::item::update_item(&search, &update)?;
            Ok(())
        }
        Err(KeyserError::ItemNotFound) => {
            // Create new item via raw SecItemAdd to set kSecAttrSynchronizable
            unsafe {
                let mut dict = CFMutableDictionary::new();
                dict.set(
                    kSecClass.cast_ref(),
                    kSecClassGenericPassword.cast_ref(),
                );
                dict.set(
                    kSecAttrService.cast_ref(),
                    CFString::new(&service).as_CFTypeRef(),
                );
                dict.set(
                    kSecAttrAccount.cast_ref(),
                    CFString::new(key).as_CFTypeRef(),
                );
                dict.set(
                    kSecValueData.cast_ref(),
                    CFData::from_buffer(value.as_bytes()).as_CFTypeRef(),
                );
                dict.set(
                    kSecAttrDescription.cast_ref(),
                    CFString::new(ITEM_DESCRIPTION).as_CFTypeRef(),
                );
                dict.set(
                    kSecAttrLabel.cast_ref(),
                    CFString::new(&label).as_CFTypeRef(),
                );
                dict.set(
                    kSecAttrSynchronizable.cast_ref(),
                    CFBoolean::false_value().as_CFTypeRef(),
                );

                let status = SecItemAdd(dict.as_concrete_TypeRef(), std::ptr::null_mut());
                if status != 0 {
                    return Err(security_framework::base::Error::from_code(status).into());
                }
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn find_item(namespace: &str, key: &str) -> Result<()> {
    let service = service_name(namespace);
    let results = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(&service)
        .account(key)
        .load_data(false)
        .search()?;
    if results.is_empty() {
        Err(KeyserError::ItemNotFound)
    } else {
        Ok(())
    }
}

pub fn search_values(namespace: &str) -> Result<Vec<(String, String)>> {
    let service = service_name(namespace);
    let results = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(&service)
        .limit(Limit::All)
        .load_attributes(true)
        .load_data(true)
        .search();

    let results = match results {
        Ok(r) => r,
        Err(e) if e.code() == errSecItemNotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    let mut pairs = Vec::new();
    for result in results {
        if let SearchResult::Dict(dict) = result {
            let account = dict
                .find(unsafe { kSecAttrAccount.cast_ref() })
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string());
            let data = dict
                .find(unsafe { kSecValueData.cast_ref() })
                .and_then(|v| v.downcast::<CFData>())
                .map(|d| String::from_utf8_lossy(d.bytes()).to_string());

            if let (Some(key), Some(value)) = (account, data) {
                pairs.push((key, value));
            }
        }
    }
    Ok(pairs)
}

pub fn search_namespaces() -> Result<Vec<String>> {
    let results = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .limit(Limit::All)
        .load_attributes(true)
        .search();

    let results = match results {
        Ok(r) => r,
        Err(e) if e.code() == errSecItemNotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    let mut names = BTreeSet::new();
    for result in results {
        if let SearchResult::Dict(dict) = result {
            let desc = dict
                .find(unsafe { kSecAttrDescription.cast_ref() })
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string());

            if desc.as_deref() != Some(ITEM_DESCRIPTION) {
                continue;
            }

            if let Some(service) = dict
                .find(unsafe { kSecAttrService.cast_ref() })
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
            {
                if let Some(ns) = service.strip_prefix(SERVICE_PREFIX) {
                    names.insert(ns.to_string());
                }
            }
        }
    }
    Ok(names.into_iter().collect())
}

pub fn delete_value(namespace: &str, key: &str) -> Result<()> {
    let service = service_name(namespace);
    let result = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(&service)
        .account(key)
        .delete();

    match result {
        Ok(()) => Ok(()),
        Err(e) if e.code() == errSecItemNotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
```

- [ ] **Step 4: Verify keychain.rs compiles**

Run:
```bash
cd ~/repos/keyser && cargo check
```

Expected: Compiles. There may be warnings about unused functions — that's fine, `main.rs` doesn't call them yet.

- [ ] **Step 5: Commit**

```bash
cd ~/repos/keyser
git add src/keychain.rs tests/integration_test.rs
git commit -m "feat: keychain module with save, search, delete, namespace listing"
```

---

### Task 3: CLI Parsing and Dispatch

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement full CLI parsing and dispatch**

Replace `src/main.rs` with:

```rust
mod keychain;

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_help(&args[0]);
        std::process::exit(2);
    }

    let exit_code = match args[1].as_str() {
        "--set" | "-s" => cmd_set(&args[0], &args[2..]),
        "--list" | "-l" => cmd_list(&args[2..]),
        "--unset" => cmd_unset(&args[0], &args[2..]),
        arg if arg.starts_with('-') => {
            eprintln!("Unknown option {arg}");
            2
        }
        _ => cmd_exec(&args[1..]),
    };

    std::process::exit(exit_code);
}

fn print_help(name: &str) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        "{name} version {version}\n\n\
         Usage:\n  \
           Add variables\n    \
             {name} (--set|-s) [--[no-]require-passphrase|-p|-P] [--noecho|-n] NAMESPACE ENV [ENV ..]\n  \
           Execute with variables\n    \
             {name} NAMESPACE CMD [ARG ...]\n  \
           List namespaces\n    \
             {name} --list\n  \
           Remove variables\n    \
             {name} --unset NAMESPACE ENV [ENV ..]\n\n\
         Options:\n  \
           --set (-s):       Add keychain item of environment variable ENV for namespace NAMESPACE.\n  \
           --noecho (-n):    Enable noecho mode when prompting values.\n  \
           --require-passphrase (-p), --no-require-passphrase (-P):\n                    \
             Replace the item's ACL list to require passphrase (or not)."
    );
}

fn cmd_set(program_name: &str, args: &[String]) -> i32 {
    let mut noecho = false;
    let mut require_passphrase: Option<bool> = None;
    let mut positional_start = 0;

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "-n" | "--noecho" => noecho = true,
            "-p" | "--require-passphrase" => require_passphrase = Some(true),
            "-P" | "--no-require-passphrase" => require_passphrase = Some(false),
            _ if arg.starts_with('-') => {
                eprintln!("Unknown option: {arg}");
                return 1;
            }
            _ => {
                positional_start = i;
                break;
            }
        }
    }

    let positional = &args[positional_start..];
    if positional.len() < 2 {
        print_help(program_name);
        std::process::exit(2);
    }

    let namespace = &positional[0];
    let keys = &positional[1..];
    let is_tty = atty::is(atty::Stream::Stdin);

    for key in keys {
        let value = if is_tty {
            ask_value_interactive(namespace, key, noecho)
        } else {
            ask_value_piped()
        };

        let value = match value {
            Some(v) => v,
            None => return 1,
        };

        if let Err(e) = keychain::save_value(namespace, key, &value, require_passphrase) {
            eprintln!("Error saving {key}: {e}");
            return 10;
        }
    }

    0
}

fn ask_value_interactive(namespace: &str, key: &str, noecho: bool) -> Option<String> {
    if noecho {
        let prompt = format!("{namespace}.{key} (noecho):");
        match rpassword::prompt_password(prompt) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("Error reading input: {e}");
                None
            }
        }
    } else {
        print!("{namespace}.{key}: ");
        io::stdout().flush().ok();
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                }
                if line.ends_with('\r') {
                    line.pop();
                }
                Some(line)
            }
            Err(e) => {
                eprintln!("Error reading input: {e}");
                None
            }
        }
    }
}

fn ask_value_piped() -> Option<String> {
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            Some(line)
        }
        Err(e) => {
            eprintln!("Error reading stdin: {e}");
            None
        }
    }
}

fn cmd_list(args: &[String]) -> i32 {
    let mut show_value = false;
    let mut namespace: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-v" | "--show-value" => show_value = true,
            _ => {
                if namespace.is_some() {
                    print_help("keyser");
                    return 2;
                }
                namespace = Some(arg);
            }
        }
    }

    if let Some(ns) = namespace {
        match keychain::search_values(ns) {
            Ok(pairs) => {
                for (key, value) in pairs {
                    if show_value {
                        println!("{key}={value}");
                    } else {
                        println!("{key}");
                    }
                }
                0
            }
            Err(e) => {
                eprintln!("Error: {e}");
                10
            }
        }
    } else {
        if show_value {
            print_help("keyser");
            return 2;
        }
        match keychain::search_namespaces() {
            Ok(names) => {
                for name in names {
                    println!("{name}");
                }
                0
            }
            Err(e) => {
                eprintln!("Error: {e}");
                10
            }
        }
    }
}

fn cmd_unset(program_name: &str, args: &[String]) -> i32 {
    if args.len() < 2 {
        print_help(program_name);
        return 2;
    }

    let namespace = &args[0];
    let keys = &args[1..];

    for key in keys {
        if let Err(e) = keychain::delete_value(namespace, key) {
            eprintln!("Error deleting {key}: {e}");
            return 10;
        }
    }

    0
}

fn cmd_exec(args: &[String]) -> i32 {
    if args.len() < 2 {
        print_help("keyser");
        return 2;
    }

    let namespaces_str = &args[0];
    let cmd = &args[1];
    let cmd_args = &args[2..];

    for namespace in namespaces_str.split(',') {
        match keychain::search_values(namespace) {
            Ok(pairs) => {
                for (key, value) in pairs {
                    std::env::set_var(&key, &value);
                }
            }
            Err(keychain::KeyserError::ItemNotFound) => {
                eprintln!(
                    "WARNING: namespace `{namespace}` not defined.\n         \
                     You can set via running `keyser --set {namespace} SOME_ENV_NAME`.\n"
                );
            }
            Err(e) => {
                eprintln!("Error: {e}");
                return 10;
            }
        }
    }

    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(cmd).args(cmd_args).exec();
    eprintln!("execvp failed: {err}");
    1
}
```

- [ ] **Step 2: Add the `atty` dependency**

The piped stdin detection needs `atty`. Add to `Cargo.toml` under `[dependencies]`:

```toml
atty = "0.2"
```

- [ ] **Step 3: Verify imports**

The imports at the top of `main.rs` should be:

```rust
use std::io::{self, BufRead, Write};
```

Both `ask_value_interactive` and `ask_value_piped` use `read_line` from `BufRead`. No `Read` import needed.

- [ ] **Step 4: Verify it compiles**

Run:
```bash
cd ~/repos/keyser && cargo build
```

Expected: Compiles with no errors.

- [ ] **Step 5: Run tests**

Run:
```bash
cd ~/repos/keyser && cargo test -- --test-threads=1
```

Expected: All integration tests pass. Tests run serially (`--test-threads=1`) to avoid Keychain race conditions.

- [ ] **Step 6: Quick manual smoke test**

Run:
```bash
cd ~/repos/keyser
echo "testval" | cargo run -- --set smoketest SMOKE_VAR
cargo run -- --list smoketest
cargo run -- smoketest env | grep SMOKE_VAR
cargo run -- --unset smoketest SMOKE_VAR
cargo run -- --list smoketest
```

Expected output:
```
SMOKE_VAR
SMOKE_VAR=testval
(empty output after unset)
```

- [ ] **Step 7: Commit**

```bash
cd ~/repos/keyser
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: CLI parsing with set, list, unset, and exec modes"
```

---

### Task 4: Edge Cases and Polish

**Files:**
- Modify: `tests/integration_test.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add test for missing namespace warning in exec mode**

Append to `tests/integration_test.rs`:

```rust
#[test]
fn test_exec_missing_namespace_warns() {
    let output = keyser_bin()
        .args(["nonexistent-ns-xyzzy", "echo", "hello"])
        .output()
        .expect("failed to run keyser exec");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("WARNING") && stderr.contains("nonexistent-ns-xyzzy"),
        "expected warning about missing namespace, got: {stderr}"
    );
    // Command should still execute
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello"), "command should still run, got: {stdout}");
}

#[test]
fn test_update_existing_value() {
    cleanup_test_namespace();

    // Set initial value
    let status = keyser_bin()
        .args(["--set", TEST_NS, "UPDATE_ME"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"original\n")?;
            child.wait()
        })
        .expect("failed to set");
    assert!(status.success());

    // Update to new value
    let status = keyser_bin()
        .args(["--set", TEST_NS, "UPDATE_ME"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"updated\n")?;
            child.wait()
        })
        .expect("failed to set");
    assert!(status.success());

    // Verify new value
    let output = keyser_bin()
        .args(["--list", TEST_NS, "-v"])
        .output()
        .expect("failed to list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("UPDATE_ME=updated"), "expected updated value, got: {stdout}");
    assert!(!stdout.contains("original"), "original value should be gone, got: {stdout}");

    cleanup_test_namespace();
}

#[test]
fn test_no_args_shows_help() {
    let output = keyser_bin()
        .output()
        .expect("failed to run keyser");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage:"), "expected help output, got: {stderr}");
}

#[test]
fn test_unset_nonexistent_is_ok() {
    let status = keyser_bin()
        .args(["--unset", "nonexistent-ns-xyzzy", "NOPE"])
        .status()
        .expect("failed to run keyser --unset");
    assert!(status.success(), "unset of nonexistent item should succeed silently");
}
```

- [ ] **Step 2: Run all tests**

Run:
```bash
cd ~/repos/keyser && cargo test -- --test-threads=1
```

Expected: All tests pass (including the new edge case tests) with no changes to `main.rs` — the behavior is already implemented.

- [ ] **Step 3: Run clippy and fmt**

Run:
```bash
cd ~/repos/keyser && cargo fmt && cargo clippy -- -D warnings
```

Expected: No warnings, no errors. Fix any issues that arise.

- [ ] **Step 4: Commit**

```bash
cd ~/repos/keyser
git add tests/integration_test.rs src/main.rs src/keychain.rs
git commit -m "test: edge cases for missing namespace, value update, no args, unset nonexistent"
```

---

### Task 5: CI/CD and Release Infrastructure

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/pr-title.yml`
- Create: `.github/workflows/release-please.yml`
- Create: `.github/workflows/release.yml`
- Create: `release-please-config.json`
- Create: `.release-please-manifest.json`
- Create: `justfile`
- Create: `install.sh`

- [ ] **Step 1: Create justfile**

Create `justfile`:

```just
# List available recipes
default:
    @just --list

# Run all checks (fmt, clippy, test)
check:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test -- --test-threads=1

# Run tests
test *args:
    cargo test -- --test-threads=1 {{args}}

# Build release binary
build:
    cargo build --release

# Run formatting
fmt:
    cargo fmt

# Install keyser locally
install:
    cargo install --path .
```

- [ ] **Step 2: Create CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
    paths:
      - "src/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "justfile"
      - ".github/workflows/ci.yml"
  pull_request:
    branches: [main]
    paths:
      - "src/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "justfile"
      - ".github/workflows/ci.yml"

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  check:
    name: Check, Lint & Test
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: extractions/setup-just@v2
      - uses: Swatinem/rust-cache@v2
      - name: Run checks
        run: just check
```

- [ ] **Step 3: Create PR title workflow**

Create `.github/workflows/pr-title.yml`:

```yaml
name: PR Title

on:
  pull_request:
    types: [opened, edited, synchronize, reopened]

jobs:
  validate:
    permissions:
      pull-requests: read
    runs-on: ubuntu-latest
    steps:
      - uses: amannn/action-semantic-pull-request@e32d7e603df1aa1ba07e981f2a23455dee596825 # v5
        env:
          GITHUB_TOKEN: ${{ github.token }}
```

- [ ] **Step 4: Create release-please workflow**

Create `.github/workflows/release-please.yml`:

```yaml
name: Release Please

on:
  push:
    branches:
      - main

permissions:
  actions: write
  contents: write
  pull-requests: write

jobs:
  release-please:
    runs-on: ubuntu-latest
    steps:
      - uses: googleapis/release-please-action@v4
        id: release-please
        with:
          config-file: release-please-config.json
          manifest-file: .release-please-manifest.json

      - name: Dispatch release build
        if: ${{ steps.release-please.outputs.releases_created == 'true' }}
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh workflow run release.yml --repo ${{ github.repository }} -f tag=${{ steps.release-please.outputs.tag_name }}
```

- [ ] **Step 5: Create release workflow**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Git tag to build and release (e.g. v0.1.2)"
        required: true
        type: string

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: aarch64-apple-darwin
            os: macos-latest
            asset: keyser-aarch64-macos.tar.gz

    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ inputs.tag }}
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar czf ../../../${{ matrix.asset }} keyser
          cd ../../..

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.asset }}
          path: ${{ matrix.asset }}

  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Upload to GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ inputs.tag }}
          files: |
            keyser-aarch64-macos.tar.gz
```

- [ ] **Step 6: Create release-please config files**

Create `release-please-config.json`:

```json
{
  "$schema": "https://raw.githubusercontent.com/googleapis/release-please/main/schemas/config.json",
  "packages": {
    ".": {
      "release-type": "rust",
      "include-component-in-tag": false,
      "bump-minor-pre-major": false,
      "bump-patch-for-minor-pre-major": true,
      "changelog-sections": [
        { "type": "feat", "section": "Features" },
        { "type": "fix", "section": "Bug Fixes" },
        { "type": "chore", "section": "Miscellaneous" },
        { "type": "docs", "section": "Documentation" },
        { "type": "refactor", "section": "Code Refactoring" }
      ]
    }
  }
}
```

Create `.release-please-manifest.json`:

```json
{
  ".": "0.1.0"
}
```

- [ ] **Step 7: Create install.sh**

Create `install.sh`:

```bash
#!/bin/sh
# Keyser installer — https://github.com/raiderrobert/keyser
# Usage: curl -fsSL https://raw.githubusercontent.com/raiderrobert/keyser/main/install.sh | sh
set -e

REPO="raiderrobert/keyser"
INSTALL_DIR="${KEYSER_INSTALL_DIR:-/usr/local/bin}"

main() {
    platform="$(detect_platform)"
    arch="$(detect_arch)"
    asset="$(asset_name "$platform" "$arch")"

    if [ -z "$asset" ]; then
        echo "Error: unsupported platform/architecture: ${platform}/${arch}" >&2
        echo "Pre-built binaries are available for:" >&2
        echo "  - macOS (Apple Silicon / aarch64)" >&2
        echo "" >&2
        echo "You can build from source instead: cargo install --path ." >&2
        exit 1
    fi

    url="https://github.com/${REPO}/releases/latest/download/${asset}"

    echo "Detected: ${platform}/${arch}"
    echo "Downloading: ${url}"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    if command -v curl > /dev/null 2>&1; then
        curl -fsSL "$url" -o "${tmpdir}/${asset}"
    elif command -v wget > /dev/null 2>&1; then
        wget -qO "${tmpdir}/${asset}" "$url"
    else
        echo "Error: curl or wget is required" >&2
        exit 1
    fi

    tar xzf "${tmpdir}/${asset}" -C "$tmpdir"

    if [ -w "$INSTALL_DIR" ]; then
        mv "${tmpdir}/keyser" "${INSTALL_DIR}/keyser"
    else
        echo "Installing to ${INSTALL_DIR} (requires sudo)"
        sudo mv "${tmpdir}/keyser" "${INSTALL_DIR}/keyser"
    fi

    chmod +x "${INSTALL_DIR}/keyser"

    echo "Installed keyser to ${INSTALL_DIR}/keyser"
    "${INSTALL_DIR}/keyser" 2>/dev/null || true
}

detect_platform() {
    case "$(uname -s)" in
        Darwin*) echo "macos" ;;
        *)       echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        arm64|aarch64) echo "aarch64" ;;
        *)             echo "unknown" ;;
    esac
}

asset_name() {
    platform="$1"
    arch="$2"

    case "${arch}-${platform}" in
        aarch64-macos) echo "keyser-aarch64-macos.tar.gz" ;;
        *)             echo "" ;;
    esac
}

main
```

- [ ] **Step 8: Verify `just check` passes**

Run:
```bash
cd ~/repos/keyser && just check
```

Expected: fmt check passes, clippy passes, all tests pass.

- [ ] **Step 9: Commit**

```bash
cd ~/repos/keyser
git add justfile install.sh release-please-config.json .release-please-manifest.json .github/
git commit -m "ci: add CI/CD workflows, justfile, release-please, and install script"
```

---

### Task 6: README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write README.md**

Create `README.md`:

````markdown
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
````

- [ ] **Step 2: Commit**

```bash
cd ~/repos/keyser
git add README.md
git commit -m "docs: add README"
```

---

### Task 7: Push and Verify

**Files:** None (git operations only)

- [ ] **Step 1: Run full check one final time**

Run:
```bash
cd ~/repos/keyser && just check
```

Expected: All checks pass.

- [ ] **Step 2: Push to GitHub**

```bash
cd ~/repos/keyser && git push origin main
```

- [ ] **Step 3: Verify CI runs on GitHub**

Run:
```bash
gh run list --repo raiderrobert/keyser --limit 3
```

Expected: A CI run triggered by the push to main. Wait for it and check status:

```bash
gh run watch --repo raiderrobert/keyser
```
