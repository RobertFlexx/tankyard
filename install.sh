#!/bin/sh
# Download a released build. Rust and administrator access are not required.
set -eu

repository=RobertFlexx/tankyard
version=latest
bin_dir="$HOME/.local/bin"
desktop=yes
component=game

die() { printf 'Tank Yard: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'HELP'
Tank Yard installer

  sh install.sh [--version v0.2.7] [--bin-dir DIR] [--no-desktop] [--server]

Installs the game into ~/.local/bin and adds an application shortcut.
Run the same command to update. --server installs the server and relay instead.
Your settings, saves, and server configuration are kept.
HELP
}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version|--bin-dir)
            [ "$#" -ge 2 ] || die "$1 needs a value"
            case "$1" in --version) version=$2 ;; --bin-dir) bin_dir=$2 ;; esac
            shift 2 ;;
        --no-desktop) desktop=no; shift ;;
        --server) component=server; desktop=no; shift ;;
        --help|-h) usage; exit 0 ;;
        *) die "Unknown option: $1. Use --help." ;;
    esac
done
[ -n "$bin_dir" ] || die "Choose an installation directory."
case "$bin_dir" in /*) ;; *) die "--bin-dir must be an absolute path." ;; esac
for tool in curl tar awk mktemp install; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is required."
done
system=$(uname -s)
machine=$(uname -m)
case "$system/$machine" in
    Linux/x86_64|Linux/amd64) platform=linux-x86_64 ;;
    Linux/aarch64|Linux/arm64) platform=linux-aarch64 ;;
    Darwin/x86_64|Darwin/arm64) platform=macos-universal ;;
    *) die "No build for $system/$machine. Windows builds are on the Releases page." ;;
esac
if [ "$system" = Linux ] && ldd --version 2>&1 | awk '/musl/ { found=1 } END { exit !found }'; then
    die "This Linux build needs glibc 2.31 or newer; musl is not supported."
fi
if command -v sha256sum >/dev/null 2>&1; then
    checksum() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
    checksum() { shasum -a 256 "$1" | awk '{print $1}'; }
else
    die "Install sha256sum or shasum to verify the download."
fi
download() {
    curl --fail --location --retry 3 --connect-timeout 15 --max-time 180 \
        --proto '=https' --tlsv1.2 --silent --show-error --output "$2" "$1"
}
if [ -t 1 ] && [ -z "$(printenv NO_COLOR || true)" ]; then
    printf '\033[1;36mTank Yard\033[0m\n'
else
    printf 'Tank Yard\n'
fi
base="https://github.com/$repository/releases"
if [ "$version" = latest ]; then
    resolved=$(curl --fail --location --retry 3 --connect-timeout 15 --max-time 60 \
        --proto '=https' --tlsv1.2 --silent --show-error --output /dev/null \
        --write-out '%{url_effective}' "$base/latest") || die "Could not find the latest release."
    version=$(printf '%s' "$resolved" | awk -F '/tag/' 'NF==2 {print $2}')
fi
case "$version" in v[0-9]*) ;; *) die "Use a release tag such as v0.2.7." ;; esac
case "$version" in *[!a-zA-Z0-9.-]*) die "Invalid release tag." ;; esac

work_dir=$(mktemp -d)
pending_file=
app_stage=
cleanup() {
    rm -rf "$work_dir"
    if [ -n "$pending_file" ]; then rm -f "$pending_file"; fi
    if [ -n "$app_stage" ]; then rm -rf "$app_stage"; fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
root_name="tankyard-$version-$platform"
archive="$root_name.tar.gz"
printf 'Downloading %s for %s…\n' "$version" "$platform"
download "$base/download/$version/SHA256SUMS" "$work_dir/SHA256SUMS" || die "Release checksums are unavailable."
expected=$(awk -v name="$archive" '$2 == name { print $1 }' "$work_dir/SHA256SUMS")
[ "$(printf '%s' "$expected" | wc -c | tr -d ' ')" = 64 ] || die "No checksum for $archive."
case "$expected" in *[!0-9a-f]*) die "Invalid checksum." ;; esac
download "$base/download/$version/$archive" "$work_dir/$archive" || die "Download failed; the existing installation was kept."
actual=$(checksum "$work_dir/$archive")
[ "$actual" = "$expected" ] || die "Checksum mismatch; the existing installation was kept."
printf 'Download verified.\n'

tar -tzf "$work_dir/$archive" > "$work_dir/files" || die "The release archive is damaged."
while IFS= read -r member; do
    case "$member" in /*|../*|*/../*) die "Invalid archive path." ;; esac
    case "$member" in "$root_name"|"$root_name/"*) ;; *) die "Unexpected archive contents." ;; esac
done < "$work_dir/files"
tar -xzf "$work_dir/$archive" -C "$work_dir"
payload="$work_dir/$root_name"
if [ "$component" = server ]; then
    binaries='tankyard-server tankyard-relay'
    first=tankyard-server
else
    binaries=tankyard
    first=tankyard
fi
for binary in $binaries; do
    [ -f "$payload/$binary" ] || die "The archive is missing $binary."
done
reported=$("$payload/$first" --version) || die "This build cannot run here. Linux requires glibc 2.31+ and desktop graphics libraries."
printf '%s\n' "$reported"
mkdir -p "$bin_dir"
for binary in $binaries; do
    pending_file="$bin_dir/.$binary.install.$$"
    install -m 755 "$payload/$binary" "$pending_file"
    mv -f "$pending_file" "$bin_dir/$binary"
    pending_file=
done
if [ "$desktop" = yes ]; then
    if [ "$system" = Darwin ]; then
        app_dir="$HOME/Applications"
        mkdir -p "$app_dir"
        # Stage the whole bundle before swapping it, including its code signature.
        app_stage=$(mktemp -d "$app_dir/.tankyard-install.XXXXXX")
        ditto "$payload/Tank Yard.app" "$app_stage/Tank Yard.app"
        codesign --verify --strict "$app_stage/Tank Yard.app"
        if [ -e "$app_dir/Tank Yard.app" ]; then
            mv "$app_dir/Tank Yard.app" "$app_stage/previous.app"
        fi
        if ! mv "$app_stage/Tank Yard.app" "$app_dir/Tank Yard.app"; then
            if [ -d "$app_stage/previous.app" ]; then
                mv "$app_stage/previous.app" "$app_dir/Tank Yard.app"
            fi
            die "Could not replace the application; the previous app was restored."
        fi
        printf 'Application: %s/Tank Yard.app\n' "$app_dir"
    else
        data_dir=$(printenv XDG_DATA_HOME || true)
        [ -n "$data_dir" ] || data_dir="$HOME/.local/share"
        mkdir -p "$data_dir/applications" "$data_dir/icons/hicolor/scalable/apps"
        install -m 644 "$payload/tankyard.svg" "$data_dir/icons/hicolor/scalable/apps/tankyard.svg"
        # Escape literal desktop Exec metacharacters, without shell expansion.
        # shellcheck disable=SC2016
        escaped=$(printf '%s' "$bin_dir/tankyard" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\$/\\$/g; s/`/\\`/g; s/%/%%/g')
        cat > "$data_dir/applications/tankyard.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Tank Yard
Exec="$escaped"
Icon=tankyard
Terminal=false
Categories=Game;ActionGame;
DESKTOP
        if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "$data_dir/applications" >/dev/null 2>&1 || true
        fi
        printf 'Added Tank Yard to your application menu.\n'
    fi
fi
printf 'Installed to %s\n' "$bin_dir"
case ":$PATH:" in
    *":$bin_dir:"*) printf 'Ready. Run %s to start.\n' "$first" ;;
    *) printf 'Run "%s/%s", or add "%s" to your PATH.\n' "$bin_dir" "$first" "$bin_dir" ;;
esac
