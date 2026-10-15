#!/usr/bin/env bash
# Run from a native desktop or xvfb-run. This verifies rendered frames and real
# offline authoritative gameplay; it does not measure or promise render speed.
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
capture_dir="${TANKYARD_CAPTURE_DIR:-${root_dir}/artifacts/gui}"
mkdir -p "$capture_dir"
cd "$root_dir"
cargo build --locked -p tankyard-client
# Keep automated match statistics and settings separate from the player profile.
smoke_config="$(mktemp -d "${TMPDIR:-/tmp}/tankyard-smoke.XXXXXX")"
export XDG_CONFIG_HOME="$smoke_config"
trap 'rm -rf "$smoke_config"' EXIT
modes=(ffa twoteams fourteams maze domination survival tag mothership capturecore bossrush deathmatch sandbox)
if [[ "${1:-}" != "--all" ]]; then modes=("${1:-ffa}"); fi
for mode in "${modes[@]}"; do
    target/debug/tankyard --offline --mode "$mode" --bots 12 \
        --smoke-frames 240 --screenshot "$capture_dir/$mode.png" \
        > "$capture_dir/$mode.log" 2>&1
    test -s "$capture_dir/$mode.png"
    cat "$capture_dir/$mode.log"
done
