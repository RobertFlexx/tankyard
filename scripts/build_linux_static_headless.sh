#!/usr/bin/env bash
# The graphical client needs the desktop's shared runtime; headless tools do not.
set -eu
root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
unset CARGO_ENCODED_RUSTFLAGS
export RUSTFLAGS='-C target-feature=+crt-static -C target-cpu=x86-64 -C strip=symbols'
exec cargo build --locked --offline --release -p tankyard-server -p tankyard-tools --bins \
    --target x86_64-unknown-linux-gnu --target-dir "$root_dir/target/static" "$@"
