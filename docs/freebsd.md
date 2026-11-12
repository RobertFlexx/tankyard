# Tank Yard on FreeBSD

Tank Yard runs natively on FreeBSD 15.0 and newer on amd64. You don't need Linux compatibility. Each release has a `freebsd-x86_64` build of the game, the dedicated server, the relay and the command-line tools. The release workflow builds it on the newest FreeBSD 15 release.

## Installing

The game plays sound through ALSA, which reaches FreeBSD's own sound system through `alsa-plugins`. Install that once, as root:

```sh
pkg install alsa-plugins
```

Then, as yourself:

```sh
fetch -o - https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
```

The installer uses `fetch(1)` from the base system, so it doesn't need curl. It checks the download's SHA-256 checksum, installs `tankyard` into `~/.local/bin`, and adds Tank Yard to your desktop's application menu. Run the same command to update; your settings and progress are kept. [Installing](installing.md#options) lists the installer's options. When it's done, start `tankyard` from the menu or a terminal.

On FreeBSD 14 or on arm64, [build from source](#building-from-source).

## The desktop

Tank Yard needs an X11 desktop with OpenGL, and any X11 window manager or desktop environment works. On Wayland, it runs through Xwayland. A desktop installed from packages already includes the libraries it loads: `libX11`, `libXi`, `libXcursor`, `libxkbcommon`, and `libGL` from Mesa or the NVIDIA driver.

### NVIDIA

Use NVIDIA's FreeBSD driver:

```sh
pkg install nvidia-driver nvidia-settings
sysrc kld_list+=nvidia-modeset
mkdir -p /usr/local/etc/X11/xorg.conf.d
cat > /usr/local/etc/X11/xorg.conf.d/20-nvidia.conf <<'EOF'
Section "Device"
    Identifier "NVIDIA"
    Driver     "nvidia"
EndSection
EOF
```

Restart, then start your desktop. To confirm the game will use the GPU, run `glxinfo -B` (from `mesa-demos`); it should name NVIDIA as the OpenGL vendor. Older GPUs need one of the legacy driver packages, such as `nvidia-driver-470`. The FreeBSD Handbook's graphics chapter covers multi-GPU and laptop setups.

The game asks the NVIDIA driver to queue just one frame ahead, as it does on Linux. This keeps input latency low with V-Sync on. You can set your own `__GL_MaxFramesAllowed` to override it. DLSS isn't available because NVIDIA publishes its DLSS runtime only for Linux and Windows. All other graphics settings work the same as on Linux.

### AMD and Intel

Install the kernel graphics driver (`pkg install drm-kmod`, then `sysrc kld_list+=amdgpu` or `kld_list+=i915kms`) along with `mesa-dri`.

## Sound and voice chat

With `alsa-plugins` installed, the game plays through FreeBSD's default sound device and records voice chat from it. To list your devices, run `cat /dev/sndstat`. To make a different device the default, run `sysctl hw.snd.default_unit=N`, or add that setting to `/etc/sysctl.conf` to keep it. Change volumes and the recording source with `mixer(8)`.

If the game says there is no audio device, check whether `alsa-plugins` is installed. If you're starting the game without sound on purpose, use `tankyard --no-audio`.

## Hosting a server

The server doesn't need `alsa-plugins` or a desktop. Install it for your user with:

```sh
fetch -o - https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh -s -- --server
```

Then follow [Hosting](hosting.md#a-dedicated-server). To start the server when the machine boots, use the rc.d script in [Hosting on FreeBSD](hosting.md#on-freebsd).

## Building from source

Install the build tools and libraries, then Rust 1.88 or newer (from `pkg install rust` or [rustup](https://rustup.rs)):

```sh
pkg install git cmake pkgconf python3 alsa-lib alsa-plugins libX11 libXi libXcursor libxkbcommon
git clone https://github.com/RobertFlexx/tankyard.git
cd tankyard
cargo build --release --locked
./target/release/tankyard
```

The base system's Clang compiles the bundled Opus and Luau sources. `cargo build --release --locked -p tankyard-server` builds only the server, which has no desktop or sound dependencies. Releases are tested only on FreeBSD 15 on amd64, but the same steps should work on arm64 and FreeBSD 14.

## For maintainers

`.github/workflows/freebsd.yml` builds, checks and packages the FreeBSD release in a FreeBSD VM on GitHub's Linux runners, using [vmactions/freebsd-vm](https://github.com/vmactions/freebsd-vm). Its `release: "15"` setting selects the newest 15.x. The Release workflow calls it for every tag, and after publishing it installs the release on FreeBSD without curl. To check FreeBSD before tagging, run **FreeBSD** from the Actions tab and download `release-freebsd-x86_64` from the run.

`scripts/build_freebsd.sh` does the build itself, on FreeBSD. It builds the release, checks that every binary reports the version and loads the bundled mods, and checks that the binaries need nothing beyond the base system and ALSA, with no symbols newer than FreeBSD 15.0. It also renders the game on a virtual display, then writes `dist/tankyard-<version>-freebsd-x86_64.tar.gz`.

To work without GitHub, run the same steps in a local QEMU VM on Linux. The first start downloads the official FreeBSD cloud image and installs the tools:

```sh
scripts/freebsd_vm.sh start      # boot FreeBSD 15.1
scripts/freebsd_vm.sh test       # run the workspace tests
scripts/freebsd_vm.sh smoke      # screenshot to artifacts/freebsd-smoke.png
scripts/freebsd_vm.sh package    # the release archive, in dist/
scripts/freebsd_vm.sh stop
```

The VM keeps its files in `~/.cache/tankyard-freebsd`; `scripts/freebsd_vm.sh destroy` deletes its disk.

## Troubleshooting

| Problem | Try |
| --- | --- |
| The installer says to install the game's sound libraries | `pkg install alsa-plugins`, as root, then run the installer again |
| `Shared object "libsys.so.7" not found` or `FBSD_1.9` errors | The release build needs FreeBSD 15.0 or newer. On older systems, [build from source](#building-from-source) |
| The window doesn't open | Check that `glxinfo -B` works in the same session. Try `tankyard --offline --safe-graphics` and include its output when reporting a problem |
| Software rendering or a slow game on NVIDIA | `glxinfo -B` names Mesa instead of NVIDIA: check that `nvidia-modeset` is loaded (`kldstat`) and that the Xorg configuration above is in place |
| No sound, or voice chat hears nothing | Check `cat /dev/sndstat` and `hw.snd.default_unit`, and set the recording source with `mixer` |
