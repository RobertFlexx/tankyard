#!/bin/sh
# Launch a community-hosted Tank Yard arena without central infrastructure.
# Usage: ./scripts/host_server.sh [additional server arguments]
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release -p tankyard-server --bin tankyard-server
exec target/release/tankyard-server --config configs/server.toml "$@"
