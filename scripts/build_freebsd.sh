#!/bin/sh
# Builds, checks and packages the FreeBSD release on FreeBSD itself:
#
#   sh scripts/build_freebsd.sh v0.15.2
#
# The release workflow runs it in GitHub's FreeBSD VM, and
# scripts/freebsd_vm.sh package runs it in a local one. It needs the packages
# listed in docs/freebsd.md#building-from-source and Rust from rustup. The
# archive lands in dist/; build files go to CARGO_TARGET_DIR, by default
# outside the checkout so a VM copies back only the package.
set -eu

[ "$#" -eq 1 ] || { echo "usage: sh scripts/build_freebsd.sh vX.Y.Z" >&2; exit 2; }
version=$1
cd "$(dirname "$0")/.."
[ "$(uname -s)/$(uname -m)" = FreeBSD/amd64 ] || { echo "Run this on FreeBSD amd64." >&2; exit 1; }
: "${CARGO_TARGET_DIR:=$HOME/tankyard-target}"
export CARGO_TARGET_DIR
export RUSTFLAGS="-C strip=symbols"
binaries=$CARGO_TARGET_DIR/release

cargo build --locked --release --workspace --bins
python3 scripts/check_release_binaries.py --binary-dir "$binaries" --version "$version" --platform freebsd-x86_64

# Players install only alsa-plugins (which brings alsa-lib) and their desktop;
# everything else must come from the base system. Symbols newer than FBSD_1.9
# would shut out FreeBSD 15.0.
for binary in tankyard tankyard-server tankyard-relay tankyard-load tankyard-tools; do
    ldd -f '%o %p\n' "$binaries/$binary" | while read -r name path; do
        # The kernel maps [vdso] into every process; it is not a file.
        case "$name" in \[*\]) continue ;; esac
        if [ "$name" = libasound.so.2 ]; then
            [ "$binary" = tankyard ] || { echo "$binary should not need $name" >&2; exit 1; }
            continue
        fi
        case "$path" in
            /lib/*|/usr/lib/*) ;;
            *) echo "$binary needs $name from $path, outside the base system" >&2; exit 1 ;;
        esac
    done
    newest=$(readelf -V "$binaries/$binary" | grep -o 'FBSD_1\.[0-9]*' | sort -t . -k 2 -n | tail -n 1)
    case "$newest" in
        FBSD_1.[0-9]) ;;
        *) echo "$binary needs $newest, newer than FreeBSD 15.0" >&2; exit 1 ;;
    esac
done
echo "Release binaries need only the base system and ALSA, as on FreeBSD 15.0"

# Start the game on a virtual display with Mesa's software renderer.
if command -v Xvfb >/dev/null 2>&1; then
    smoke=$(mktemp -d)
    Xvfb :97 -screen 0 1280x800x24 -nolisten tcp >"$smoke/xvfb.log" 2>&1 &
    xvfb=$!
    sleep 3
    status=0
    env DISPLAY=:97 LIBGL_ALWAYS_SOFTWARE=1 XDG_CONFIG_HOME="$smoke/config" \
        "$binaries/tankyard" --offline --bots 4 --smoke-frames 120 --screenshot "$smoke/smoke.png" || status=$?
    kill "$xvfb" 2>/dev/null || true
    [ "$status" -eq 0 ] && [ -s "$smoke/smoke.png" ] || { echo "The game did not render on FreeBSD" >&2; exit 1; }
    rm -rf "$smoke"
    echo "The game renders on FreeBSD"
fi

python3 scripts/package_release.py --platform freebsd-x86_64 --version "$version" --binary-dir "$binaries"
