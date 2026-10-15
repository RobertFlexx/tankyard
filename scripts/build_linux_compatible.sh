#!/bin/sh
# Keep Rust dependencies in the executable and use the desktop's glibc/GPU runtime.
set -eu
root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
command -v "${TANKYARD_ZIG:-zig}" >/dev/null
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$root_dir/scripts/link_linux_compatible.sh"
unset CARGO_ENCODED_RUSTFLAGS
export RUSTFLAGS='-C target-feature=-crt-static -C target-cpu=x86-64 -C strip=symbols'
exec cargo build --locked --offline --release --workspace --bins \
    --target x86_64-unknown-linux-gnu --target-dir "$root_dir/target/linux-compatible" "$@"
