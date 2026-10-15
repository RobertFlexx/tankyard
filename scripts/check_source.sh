#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
    printf '%s\n' 'Cargo is required. Install Rust with rustup.' >&2
    exit 127
fi
printf '%s\n' 'Checking formatting...'
cargo fmt --all -- --check
printf '%s\n' 'Checking code...'
cargo check --locked --workspace --all-targets
printf '%s\n' 'Running tests...'
cargo test --locked --workspace
printf '%s\n' 'Running Clippy...'
cargo clippy --locked --workspace --all-targets -- -D warnings
