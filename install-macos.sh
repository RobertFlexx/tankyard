#!/bin/sh
# Install Tank Yard on macOS from the release .pkg without the Gatekeeper
# block that affects packages not notarized by Apple. It opens the normal
# visual Installer; --cli installs from the terminal instead.
#
#   curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install-macos.sh | sh
#
# Browsers tag downloads with a quarantine flag that Gatekeeper checks; curl
# does not. This script downloads the package with curl, verifies it against
# the release's published SHA-256 checksums, and installs it with Apple's own
# Installer app. With --file it verifies a package you already downloaded first,
# and only then clears its quarantine flag. A file that fails verification is
# never unblocked or installed.
set -eu

repository=RobertFlexx/tankyard
version=latest
file=
open_app=yes
cli=no

die() { printf 'Tank Yard: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'HELP'
Tank Yard macOS installer

  sh install-macos.sh [--version v0.10.0] [--file PATH] [--cli] [--no-open]

Downloads the release .pkg, verifies its checksum and opens it in the macOS
Installer, which installs Tank Yard.app into /Applications. Run the same
command to update; settings and mods are kept.

  --file PATH   Use a .pkg you already downloaded, after verifying it.
  --cli         Install from the terminal instead of the visual Installer.
  --no-open     With --cli, don't open Tank Yard afterwards.
HELP
}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version|--file)
            [ "$#" -ge 2 ] || die "$1 needs a value"
            case "$1" in --version) version=$2 ;; --file) file=$2 ;; esac
            shift 2 ;;
        --no-open) open_app=no; shift ;;
        --cli) cli=yes; shift ;;
        --help|-h) usage; exit 0 ;;
        *) die "Unknown option: $1. Use --help." ;;
    esac
done
[ "$(uname -s)" = Darwin ] || die "This installer is for macOS. On Linux, use install.sh."
for tool in curl shasum awk installer mktemp; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is required."
done
# fetch URL FILE [progress]: download over HTTPS only. GitHub's release
# servers sometimes answer 502/503/504 or drop connections under load, so
# those are retried with growing pauses and partial downloads resume.
# Missing files (404) fail at once. Stalled transfers are abandoned after
# 30 seconds below 1 KB/s instead of after a fixed total time.
attempts=$(printenv TANKYARD_DOWNLOAD_ATTEMPTS || true)
case "$attempts" in ''|*[!0-9]*) attempts=6 ;; esac
fetch() {
    try=1
    pause=2
    meter=--silent
    if [ "${3:-}" = progress ] && [ -t 2 ]; then meter=--progress-bar; fi
    while :; do
        resume=
        if [ -s "$2" ]; then resume='--continue-at -'; fi
        # shellcheck disable=SC2086
        code=$(curl --fail --location --connect-timeout 10 --speed-limit 1024 --speed-time 30 \
            --proto '=https' --proto-redir '=https' --tlsv1.2 $meter --show-error \
            $resume --write-out '%{http_code}' --output "$2" "$1") && return 0
        status=$?
        case "$code" in
            416) rm -f "$2" ;;    # A stale partial file; start again.
            ''|000|408|429|5??) ;;
            *) return "$status" ;; # 404 and other permanent answers.
        esac
        # curl reports HTTP errors as 22; without a status it never connected.
        if [ -z "$code" ] && [ "$status" = 22 ]; then return "$status"; fi
        [ "$try" -lt "$attempts" ] || return "$status"
        printf 'Tank Yard: the download server is busy (%s); retrying in %ss…\n' \
            "${code:-no response}" "$pause" >&2
        sleep "$pause"
        try=$((try + 1))
        pause=$((pause * 2))
        [ "$pause" -le 30 ] || pause=30
    done
}
base="https://github.com/$repository/releases"
if [ -n "$file" ]; then
    [ -f "$file" ] || die "$file does not exist."
    # The file name carries its version: tankyard-vX.Y.Z-macos-universal.pkg.
    if [ "$version" = latest ]; then
        version=$(basename "$file" | awk -F- '/^tankyard-v[0-9][0-9.]*-macos-universal\.pkg$/ { print $2 }')
        [ -n "$version" ] || die "Use --version, or keep the original file name (tankyard-vX.Y.Z-macos-universal.pkg)."
    fi
fi
case "$version" in latest|v[0-9]*) ;; *) die "Use a release tag such as v0.10.0." ;; esac
case "$version" in *[!a-zA-Z0-9.-]*) die "Invalid release tag." ;; esac

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
if [ "$version" = latest ]; then
    # The latest checksum list names the package, and with it the version.
    fetch "$base/latest/download/SHA256SUMS" "$work_dir/SHA256SUMS" \
        || die "Could not reach the latest release. Check your connection, or try again in a minute."
    version=$(awk '$2 ~ /^tankyard-v[0-9][0-9.]*-macos-universal\.pkg$/ {
        split($2, part, "-"); print part[2]; exit }' "$work_dir/SHA256SUMS")
    case "$version" in v[0-9]*) ;; *) die "The latest release has no macOS installer package." ;; esac
else
    fetch "$base/download/$version/SHA256SUMS" "$work_dir/SHA256SUMS" \
        || die "Release $version or its checksums are unavailable."
fi
package="tankyard-$version-macos-universal.pkg"
printf 'Tank Yard %s for macOS\n' "$version"
expected=$(awk -v name="$package" '$2 == name { print $1 }' "$work_dir/SHA256SUMS")
[ "$(printf '%s' "$expected" | wc -c | tr -d ' ')" = 64 ] || die "Release $version has no macOS installer package."
case "$expected" in *[!0-9a-f]*) die "Invalid checksum." ;; esac
if [ -z "$file" ]; then
    if [ "$cli" = yes ]; then
        file="$work_dir/$package"
    else
        # The Installer app reads the package after this script exits.
        mkdir -p "$HOME/Downloads"
        file="$HOME/Downloads/$package"
    fi
    if [ -s "$file" ] && [ "$(shasum -a 256 "$file" | awk '{print $1}')" = "$expected" ]; then
        printf 'Using the verified package already in %s.\n' "$file"
    else
        printf 'Downloading %s…\n' "$package"
        # An old or damaged copy is replaced; retries within this run resume.
        rm -f "$file"
        fetch "$base/download/$version/$package" "$file" progress \
            || die "Download failed; nothing was changed. Run the installer again to retry."
    fi
fi
actual=$(shasum -a 256 "$file" | awk '{print $1}')
[ "$actual" = "$expected" ] || die "$file does not match the official $version package. It was not unblocked or installed."
printf 'Verified against the official release checksum.\n'
# Only a verified package is unblocked. Missing flags are fine.
xattr -d com.apple.quarantine "$file" 2>/dev/null || true
if [ "$cli" = no ]; then
    printf 'Opening the installer. Follow its steps to finish.\n'
    open "$file" || die "Could not open the Installer. Run again with --cli to install from the terminal."
    exit 0
fi
printf 'Installing (macOS asks for your password)…\n'
sudo installer -pkg "$file" -target / || die "The installer did not finish; nothing else was changed."
app="/Applications/Tank Yard.app"
[ -d "$app" ] || die "Installation finished, but $app is missing."
# Belt and braces: an app installed from a verified package needs no flag.
sudo xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
printf 'Installed %s\n' "$app"
if [ "$open_app" = yes ]; then
    open "$app" || true
fi
