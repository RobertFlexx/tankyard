#!/bin/sh
# Keep Rust dependencies in the executable and use the desktop's glibc/GPU runtime.
set -eu
root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
command -v "${TANKYARD_ZIG:-zig}" >/dev/null
build_target=${TANKYARD_BUILD_TARGET:-x86_64-unknown-linux-gnu}
case "$build_target" in
    x86_64-unknown-linux-gnu)
        export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$root_dir/scripts/link_linux_compatible.sh"
        export TANKYARD_ZIG_TARGET=x86_64-linux-gnu.2.31
        build_cpu=x86-64 ;;
    aarch64-unknown-linux-gnu)
        export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$root_dir/scripts/link_linux_compatible.sh"
        export TANKYARD_ZIG_TARGET=aarch64-linux-gnu.2.31
        build_cpu=generic ;;
    *) printf 'Unsupported Linux target: %s\n' "$build_target" >&2; exit 1 ;;
esac
unset CARGO_ENCODED_RUSTFLAGS
export RUSTFLAGS="-C target-feature=-crt-static -C target-cpu=$build_cpu -C strip=symbols"
exec cargo build --locked --offline --release --workspace --bins \
    --target "$build_target" --target-dir "$root_dir/target/linux-compatible" "$@"
