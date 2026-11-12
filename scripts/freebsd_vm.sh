#!/usr/bin/env bash
# Builds and tests Tank Yard on FreeBSD in a local QEMU virtual machine, the
# same way the release workflow does on GitHub. Linux with KVM is fastest;
# without KVM the VM still runs, slowly.
#
#   scripts/freebsd_vm.sh start     download FreeBSD, then boot it (first boot installs the tools)
#   scripts/freebsd_vm.sh build     copy this checkout in and build release binaries
#   scripts/freebsd_vm.sh test      run the workspace tests
#   scripts/freebsd_vm.sh smoke     start the game on a virtual display and fetch a screenshot
#   scripts/freebsd_vm.sh package   build, check and package as a release: dist/tankyard-<version>-freebsd-x86_64.tar.gz
#   scripts/freebsd_vm.sh ssh [cmd] open a shell, or run a command, as root in the VM
#   scripts/freebsd_vm.sh stop      shut the VM down
#   scripts/freebsd_vm.sh destroy   stop it and delete its disk (the download is kept)
#
# Settings: TANKYARD_FREEBSD_RELEASE (default 15.1), TANKYARD_FREEBSD_DIR
# (default ~/.cache/tankyard-freebsd), TANKYARD_FREEBSD_CPUS, _MEMORY (MB),
# _DISK and _SSH_PORT.
# shellcheck disable=SC2016 # single-quoted commands run in the VM
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
RELEASE=${TANKYARD_FREEBSD_RELEASE:-15.1}
DIR=${TANKYARD_FREEBSD_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/tankyard-freebsd}
CPUS=${TANKYARD_FREEBSD_CPUS:-$(($(nproc) > 2 ? $(nproc) - 1 : 2))}
MEMORY=${TANKYARD_FREEBSD_MEMORY:-6144}
DISK=${TANKYARD_FREEBSD_DISK:-40G}
PORT=${TANKYARD_FREEBSD_SSH_PORT:-2222}
IMAGE="FreeBSD-$RELEASE-RELEASE-amd64-BASIC-CLOUDINIT-ufs.qcow2"
MIRROR="https://download.freebsd.org/releases/VM-IMAGES/$RELEASE-RELEASE/amd64/Latest"
VM="$DIR/$RELEASE"
KEY="$DIR/id_ed25519"
# Packages for building, testing and the smoke test (Xvfb with Mesa's
# software renderer). Players need only the runtime set; see docs/freebsd.md.
PACKAGES="bash curl cmake pkgconf git python3 alsa-lib alsa-plugins libX11 libXi libXcursor libxkbcommon mesa-libs mesa-dri xorg-vfbserver"

die() {
	echo "freebsd_vm: $*" >&2
	exit 1
}

vm_ssh() {
	ssh -i "$KEY" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
		-o LogLevel=ERROR -o ConnectTimeout=5 root@127.0.0.1 "$@"
}

running() {
	test -f "$VM/qemu.pid" && kill -0 "$(cat "$VM/qemu.pid")" 2>/dev/null
}

fetch() {
	mkdir -p "$DIR"
	test -f "$DIR/$IMAGE" && return
	echo "Downloading FreeBSD $RELEASE"
	curl -fL --retry 3 --connect-timeout 20 -o "$DIR/$IMAGE.xz.part" "$MIRROR/$IMAGE.xz"
	expected=$(curl -fsSL --retry 3 "$MIRROR/CHECKSUM.SHA256" | sed -n "s/^SHA256 ($IMAGE.xz) = //p")
	test -n "$expected" || die "the mirror lists no checksum for $IMAGE.xz"
	echo "$expected  $DIR/$IMAGE.xz.part" | sha256sum -c - >/dev/null || die "checksum mismatch for $IMAGE.xz"
	xz -d -c "$DIR/$IMAGE.xz.part" >"$DIR/$IMAGE.part"
	mv "$DIR/$IMAGE.part" "$DIR/$IMAGE"
	rm -f "$DIR/$IMAGE.xz.part"
}

# A cloud-init seed: nuageinit, in FreeBSD's cloud images, lets root in with
# this machine's key on first boot.
seed() {
	local seed="$VM/seed"
	rm -rf "$seed"
	mkdir -p "$seed"
	printf 'instance-id: tankyard-%s\nlocal-hostname: tankyard-freebsd\n' "$RELEASE" >"$seed/meta-data"
	cat >"$seed/user-data" <<EOF
#cloud-config
ssh_authorized_keys:
  - $(cat "$KEY.pub")
runcmd:
  - mkdir -p /root/.ssh
  - echo '$(cat "$KEY.pub")' > /root/.ssh/authorized_keys
  - chmod 700 /root/.ssh
  - chmod 600 /root/.ssh/authorized_keys
  - echo 'PermitRootLogin prohibit-password' >> /etc/ssh/sshd_config
  - sysrc sshd_enable=YES
  - service sshd restart
EOF
	xorriso -as mkisofs -quiet -V cidata -J -r -o "$VM/seed.iso" "$seed" 2>"$VM/seed.log" ||
		die "could not write the cloud-init seed; see $VM/seed.log"
}

start() {
	command -v qemu-system-x86_64 >/dev/null || die "install QEMU first (qemu-system-x86_64)"
	command -v xorriso >/dev/null || die "install xorriso first; it writes the cloud-init seed"
	running && return
	fetch
	mkdir -p "$VM"
	test -f "$KEY" || ssh-keygen -q -t ed25519 -N '' -C tankyard-freebsd-vm -f "$KEY"
	if ! test -f "$VM/disk.qcow2"; then
		qemu-img create -q -f qcow2 -F qcow2 -b "$DIR/$IMAGE" "$VM/disk.qcow2" "$DISK"
		seed
	fi
	local accel=(-accel tcg)
	if test -w /dev/kvm; then
		accel=(-accel kvm -cpu host)
	else
		echo "No access to /dev/kvm; FreeBSD will run slowly."
	fi
	qemu-system-x86_64 "${accel[@]}" -smp "$CPUS" -m "$MEMORY" -display none -daemonize \
		-pidfile "$VM/qemu.pid" -serial "file:$VM/console.log" \
		-drive "if=virtio,file=$VM/disk.qcow2,format=qcow2,discard=unmap" \
		-drive "if=virtio,file=$VM/seed.iso,format=raw,media=cdrom,readonly=on" \
		-netdev "user,id=net,hostfwd=tcp:127.0.0.1:$PORT-:22" -device virtio-net-pci,netdev=net \
		-audiodev none,id=sound -device intel-hda -device hda-duplex,audiodev=sound
	echo "Booting FreeBSD $RELEASE (console log: $VM/console.log)"
	for _ in $(seq 120); do
		if vm_ssh true 2>/dev/null; then
			break
		fi
		sleep 5
	done
	vm_ssh true || die "FreeBSD did not answer over SSH; see $VM/console.log"
	if ! test -f "$VM/provisioned"; then
		echo "Installing build tools in the VM"
		# shellcheck disable=SC2086 # the package list is meant to split
		vm_ssh "env ASSUME_ALWAYS_YES=yes pkg bootstrap -f >/dev/null && pkg install -y $PACKAGES" >"$VM/provision.log" ||
			die "package installation failed; see $VM/provision.log"
		vm_ssh 'curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable -c rustfmt,clippy' >>"$VM/provision.log" 2>&1 ||
			die "Rust installation failed; see $VM/provision.log"
		touch "$VM/provisioned"
	fi
	vm_ssh 'freebsd-version; . ~/.cargo/env; rustc --version'
}

# Copies the checkout in: tracked files plus new ones Git doesn't ignore, so
# credentials and local packs stay out.
sync() {
	running || start
	(cd "$ROOT" && git ls-files -z --cached --others --exclude-standard |
		xargs -0 sh -c 'for f; do test -e "$f" && printf "%s\0" "$f"; done' sh |
		tar --null -T - -czf -) |
		vm_ssh 'rm -rf /root/tankyard && mkdir /root/tankyard && tar -xzf - -C /root/tankyard'
}

in_vm() {
	vm_ssh "cd /root/tankyard && . ~/.cargo/env && export CARGO_TARGET_DIR=/root/target && $*"
}

build() {
	sync
	in_vm 'cargo build --locked --release --workspace --bins && for b in tankyard tankyard-server tankyard-relay tankyard-load tankyard-tools; do /root/target/release/$b --version; done'
}

package() {
	sync
	local version
	version="v$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
	in_vm "sh scripts/build_freebsd.sh $version"
	mkdir -p "$ROOT/dist"
	vm_ssh "cd /root/tankyard/dist && tar -cf - tankyard-$version-freebsd-x86_64.tar.gz tankyard-$version-freebsd-x86_64.tar.gz.sha256" |
		tar -xf - -C "$ROOT/dist"
	echo "Wrote dist/tankyard-$version-freebsd-x86_64.tar.gz"
}

case "${1:-}" in
fetch) fetch ;;
start) start ;;
build) build ;;
test)
	sync
	in_vm 'cargo test --locked --workspace'
	;;
smoke)
	running || die "start and build first"
	in_vm 'rm -f /tmp/smoke.png; (Xvfb :99 -screen 0 1280x800x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &) && sleep 2 &&
		env DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 XDG_CONFIG_HOME=/tmp/tankyard-smoke /root/target/release/tankyard --offline --bots 4 --smoke-frames 120 --screenshot /tmp/smoke.png;
		status=$?; pkill Xvfb; test $status -eq 0 && test -s /tmp/smoke.png'
	mkdir -p "$ROOT/artifacts"
	vm_ssh 'cat /tmp/smoke.png' >"$ROOT/artifacts/freebsd-smoke.png"
	echo "Wrote artifacts/freebsd-smoke.png"
	;;
package) package ;;
ssh)
	shift
	vm_ssh -t "$@"
	;;
stop)
	if running; then
		vm_ssh 'shutdown -p now' || true
		for _ in $(seq 60); do running || break; sleep 1; done
		running && kill "$(cat "$VM/qemu.pid")"
	fi
	rm -f "$VM/qemu.pid"
	;;
destroy)
	"$0" stop
	rm -rf "$VM"
	;;
*)
	sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
	exit 2
	;;
esac
