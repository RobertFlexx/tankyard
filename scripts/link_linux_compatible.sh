#!/bin/sh
set -eu
root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
export ZIG_GLOBAL_CACHE_DIR="${TANKYARD_ZIG_CACHE_DIR:-$root_dir/target/zig-cache}"
export ZIG_LOCAL_CACHE_DIR="${TANKYARD_ZIG_LOCAL_CACHE_DIR:-$root_dir/target/zig-local-cache}"
# Link to ALSA's stable system ABI. A newer build host's libasound can
# reference newer glibc internally; the destination uses its own libasound.
# Keep checking unresolved symbols in the executable itself.
for argument do
    if [ "$argument" = "-lasound" ]; then
        set -- "$@" -Wl,--allow-shlib-undefined
        break
    fi
done
exec "${TANKYARD_ZIG:-zig}" cc -target x86_64-linux-gnu.2.31 "$@"
