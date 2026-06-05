#![cfg(feature = "keychain-tests")]

use std::process::{Command, Stdio};
use std::io::Write;

/// Path to the compiled binary. `cargo test` builds it at this location.
fn keyser_bin() -> String {
    // Use the CARGO_BIN_EXE_keyser env var if available (set by cargo test),
    // otherwise fall back to cargo build and locate the binary.
    env!("CARGO_BIN_EXE_keyser").to_string()
}

const TEST_NAMESPACE: &str = "keyser-integration-test";
const TEST_NAMESPACE_2: &str = "keyser-integration-test-2";

/// Clean up all keys in the test namespace.
fn cleanup_namespace(ns: &str) {
    // List keys in the namespace, then unset each one.
    let output = Command::new(keyser_bin())
        .args(["--list", ns])
        .output()
        .expect("failed to run keyser --list");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let keys: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    if !keys.is_empty() {
        let mut args = vec!["--unset".to_string(), ns.to_string()];
        args.extend(keys.iter().map(|k| k.to_string()));
        Command::new(keyser_bin())
            .args(&args)
            .output()
            .expect("failed to run keyser --unset");
    }
}

fn cleanup() {
    cleanup_namespace(TEST_NAMESPACE);
    cleanup_namespace(TEST_NAMESPACE_2);
}

/// Helper: pipe a value to `keyser --set NAMESPACE KEY`
fn set_value(ns: &str, key: &str, value: &str) {
    let mut child = Command::new(keyser_bin())
        .args(["--set", ns, key])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn keyser --set");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        writeln!(stdin, "{}", value).expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait on keyser");
    assert!(
        output.status.success(),
        "keyser --set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_set_and_list_keys() {
    cleanup();

    set_value(TEST_NAMESPACE, "TEST_VAR_A", "hello_a");
    set_value(TEST_NAMESPACE, "TEST_VAR_B", "hello_b");

    // List keys (no -v)
    let output = Command::new(keyser_bin())
        .args(["--list", TEST_NAMESPACE])
        .output()
        .expect("failed to run keyser --list");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("TEST_VAR_A"),
        "Expected TEST_VAR_A in output: {stdout}"
    );
    assert!(
        stdout.contains("TEST_VAR_B"),
        "Expected TEST_VAR_B in output: {stdout}"
    );
    // Without -v, should NOT show values
    assert!(
        !stdout.contains("hello_a"),
        "Should not show values without -v: {stdout}"
    );

    // List keys with -v
    let output = Command::new(keyser_bin())
        .args(["--list", "-v", TEST_NAMESPACE])
        .output()
        .expect("failed to run keyser --list -v");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("TEST_VAR_A=hello_a"),
        "Expected TEST_VAR_A=hello_a in output: {stdout}"
    );
    assert!(
        stdout.contains("TEST_VAR_B=hello_b"),
        "Expected TEST_VAR_B=hello_b in output: {stdout}"
    );

    cleanup();
}

#[test]
fn test_set_and_exec() {
    cleanup();

    set_value(TEST_NAMESPACE, "KEYSER_EXEC_VAR", "exec_value_123");

    // Exec: run `env` and check the variable is present
    let output = Command::new(keyser_bin())
        .args([TEST_NAMESPACE, "env"])
        .output()
        .expect("failed to run keyser exec");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("KEYSER_EXEC_VAR=exec_value_123"),
        "Expected KEYSER_EXEC_VAR=exec_value_123 in env output: {stdout}"
    );

    cleanup();
}

#[test]
fn test_unset() {
    cleanup();

    set_value(TEST_NAMESPACE, "KEYSER_UNSET_VAR", "to_be_deleted");

    // Verify it exists
    let output = Command::new(keyser_bin())
        .args(["--list", TEST_NAMESPACE])
        .output()
        .expect("failed to run keyser --list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("KEYSER_UNSET_VAR"),
        "Expected KEYSER_UNSET_VAR before unset: {stdout}"
    );

    // Unset
    let output = Command::new(keyser_bin())
        .args(["--unset", TEST_NAMESPACE, "KEYSER_UNSET_VAR"])
        .output()
        .expect("failed to run keyser --unset");
    assert!(
        output.status.success(),
        "keyser --unset failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify it's gone
    let output = Command::new(keyser_bin())
        .args(["--list", TEST_NAMESPACE])
        .output()
        .expect("failed to run keyser --list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("KEYSER_UNSET_VAR"),
        "KEYSER_UNSET_VAR should be gone after unset: {stdout}"
    );

    cleanup();
}

#[test]
fn test_list_namespaces() {
    cleanup();

    set_value(TEST_NAMESPACE, "NS_TEST_VAR", "ns_val");
    // Set same var again to test dedup
    set_value(TEST_NAMESPACE, "NS_TEST_VAR_2", "ns_val_2");

    // List namespaces (no args to --list)
    let output = Command::new(keyser_bin())
        .args(["--list"])
        .output()
        .expect("failed to run keyser --list");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(TEST_NAMESPACE),
        "Expected namespace '{}' in output: {stdout}",
        TEST_NAMESPACE
    );

    // Check no duplicates
    let count = stdout.lines().filter(|l| l.trim() == TEST_NAMESPACE).count();
    assert_eq!(count, 1, "Namespace should appear exactly once, got {count}");

    cleanup();
}

#[test]
fn test_exec_multiple_namespaces() {
    cleanup();

    set_value(TEST_NAMESPACE, "MULTI_NS_A", "val_from_ns1");
    set_value(TEST_NAMESPACE_2, "MULTI_NS_B", "val_from_ns2");

    // Exec with comma-separated namespaces
    let ns_arg = format!("{},{}", TEST_NAMESPACE, TEST_NAMESPACE_2);
    let output = Command::new(keyser_bin())
        .args([&ns_arg, "env"])
        .output()
        .expect("failed to run keyser exec with multiple namespaces");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("MULTI_NS_A=val_from_ns1"),
        "Expected MULTI_NS_A=val_from_ns1 in env output: {stdout}"
    );
    assert!(
        stdout.contains("MULTI_NS_B=val_from_ns2"),
        "Expected MULTI_NS_B=val_from_ns2 in env output: {stdout}"
    );

    cleanup();
}
