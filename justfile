# List available recipes
default:
    @just --list

# Run all checks (fmt, clippy, test)
check:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test

# Run tests
test *args:
    cargo test {{args}}

# Build release binary
build:
    cargo build --release

# Run formatting
fmt:
    cargo fmt

# Install keyser locally
install:
    cargo install --path .
