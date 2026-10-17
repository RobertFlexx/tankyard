#!/bin/sh
set -eu
root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
export ZIG_GLOBAL_CACHE_DIR="${TANKYARD_ZIG_CACHE_DIR:-$root_dir/target/zig-cache}"
export ZIG_LOCAL_CACHE_DIR="${TANKYARD_ZIG_LOCAL_CACHE_DIR:-$root_dir/target/zig-local-cache}"
exec "${TANKYARD_ZIG:-zig}" cc -target "${TANKYARD_ZIG_TARGET:-x86_64-linux-gnu.2.31}" "$@"
