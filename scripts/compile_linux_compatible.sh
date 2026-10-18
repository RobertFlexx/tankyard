#!/bin/sh
# cc-rs passes a Rust target triple; Zig uses a different Linux target spelling.
set -eu
root_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
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
exec "$root_dir/scripts/link_linux_compatible.sh" "$@"
