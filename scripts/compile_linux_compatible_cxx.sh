#!/bin/sh
# C++ for the compatible Linux build (Luau). cc-rs passes a Rust target triple;
# Zig uses a different Linux target spelling and links its bundled libc++.
set -eu
root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
export ZIG_GLOBAL_CACHE_DIR="${TANKYARD_ZIG_CACHE_DIR:-$root_dir/target/zig-cache}"
export ZIG_LOCAL_CACHE_DIR="${TANKYARD_ZIG_LOCAL_CACHE_DIR:-$root_dir/target/zig-local-cache}"
argument_count=$#
while [ "$argument_count" -gt 0 ]; do
    argument=$1
    shift
    case "$argument" in
        --target=x86_64-unknown-linux-gnu) ;;
        *) set -- "$@" "$argument" ;;
    esac
    argument_count=$((argument_count - 1))
done
exec "${TANKYARD_ZIG:-zig}" c++ -target x86_64-linux-gnu.2.31 "$@"
