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
download() {
    curl --fail --location --retry 3 --connect-timeout 15 --max-time 600 \
        --proto '=https' --tlsv1.2 --silent --show-error --output "$2" "$1"
}
base="https://github.com/$repository/releases"
if [ -n "$file" ]; then
    [ -f "$file" ] || die "$file does not exist."
    # The file name carries its version: tankyard-vX.Y.Z-macos-universal.pkg.
    if [ "$version" = latest ]; then
        version=$(basename "$file" | awk -F- '/^tankyard-v[0-9][0-9.]*-macos-universal\.pkg$/ { print $2 }')
        [ -n "$version" ] || die "Use --version, or keep the original file name (tankyard-vX.Y.Z-macos-universal.pkg)."
    fi
elif [ "$version" = latest ]; then
    resolved=$(curl --fail --location --retry 3 --connect-timeout 15 --max-time 60 \
        --proto '=https' --tlsv1.2 --silent --show-error --output /dev/null \
        --write-out '%{url_effective}' "$base/latest") || die "Could not find the latest release."
    version=$(printf '%s' "$resolved" | awk -F '/tag/' 'NF==2 {print $2}')
fi
case "$version" in v[0-9]*) ;; *) die "Use a release tag such as v0.10.0." ;; esac
case "$version" in *[!a-zA-Z0-9.-]*) die "Invalid release tag." ;; esac

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
package="tankyard-$version-macos-universal.pkg"
printf 'Tank Yard %s for macOS\n' "$version"
download "$base/download/$version/SHA256SUMS" "$work_dir/SHA256SUMS" || die "Release checksums are unavailable."
expected=$(awk -v name="$package" '$2 == name { print $1 }' "$work_dir/SHA256SUMS")
[ "$(printf '%s' "$expected" | wc -c | tr -d ' ')" = 64 ] || die "Release $version has no macOS installer package."
case "$expected" in *[!0-9a-f]*) die "Invalid checksum." ;; esac
if [ -z "$file" ]; then
    printf 'Downloading %s…\n' "$package"
    if [ "$cli" = yes ]; then
        file="$work_dir/$package"
    else
        # The Installer app reads the package after this script exits.
        mkdir -p "$HOME/Downloads"
        file="$HOME/Downloads/$package"
    fi
    download "$base/download/$version/$package" "$file" || die "Download failed; nothing was changed."
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
