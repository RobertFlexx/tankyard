# Tank Yard

[![Release](https://img.shields.io/github/v/release/RobertFlexx/tankyard?style=flat-square)](https://github.com/RobertFlexx/tankyard/releases/latest)
[![Build](https://github.com/RobertFlexx/tankyard/actions/workflows/release.yml/badge.svg)](https://github.com/RobertFlexx/tankyard/actions/workflows/release.yml)
[![License: MIT or Apache-2.0](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-64748b?style=flat-square)](LICENSE)

A tank arena by RobertFlexx, inspired by Diep.io and Arras.io. Play online, host a game, or practice offline.

**[Download](https://github.com/RobertFlexx/tankyard/releases/latest)** · [Installation](docs/installing.md) · [How to play](docs/playing.md) · [Hosting](docs/hosting.md) · [Modding](docs/modding.md)

Mods are scripted in [Luau](https://luau.org); packs written for the earlier LuaJIT sandbox still run unchanged.

Choose **Play** for the public server or **Play offline** for a local game.

Windows (64-bit): [NSIS installer (.exe)](https://github.com/RobertFlexx/tankyard/releases/download/v0.16.0/tankyard-v0.16.0-windows-x86_64-setup.exe) · [Portable ZIP](https://github.com/RobertFlexx/tankyard/releases/download/v0.16.0/tankyard-v0.16.0-windows-x86_64.zip). For the portable version, extract the ZIP and open `tankyard.exe`.

macOS 11+ (Apple silicon and Intel): [Installer package (.pkg)](https://github.com/RobertFlexx/tankyard/releases/download/v0.16.0/tankyard-v0.16.0-macos-universal.pkg). The package isn't notarized by Apple yet, so macOS blocks it when it comes from a browser. This command downloads it, checks it against the release checksums and opens the normal installer without the warning:

```sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install-macos.sh | sh
```

Already downloaded the `.pkg`? Run `sh install-macos.sh --file ~/Downloads/tankyard-v0.16.0-macos-universal.pkg` to verify it and open the installer. You can also open **System Settings → Privacy & Security** after macOS blocks it and click **Open Anyway**.

Install or update on Linux and macOS:

```sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
```

FreeBSD 15+ (amd64): run `pkg install alsa-plugins` as root, then:

```sh
fetch -o - https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
```

[Tank Yard on FreeBSD](docs/freebsd.md) covers NVIDIA, sound and running a server.

Build from source with Rust 1.88 or newer:

```sh
cargo build --locked --release --workspace
./target/release/tankyard
```

See [Linux build requirements](docs/linux-builds.md), [building on FreeBSD](docs/freebsd.md#building-from-source) and [third-party notices](NOTICE.md).

## Tank you!

Tank Yard has been years in the making. Thank you to everyone who’s played, shared ideas, and supported it.

entirely made by one guy. (me! :D)

> dual licensed, pick what you wanna abide by :D

The [Arena Workshop](docs/map-editor.md) builds custom maps in the game, with pads, conveyors, portals, particle effects, and map logic you can test in place: watchers, sequences and AND, OR and XOR gates. [Hosting](docs/hosting.md) covers a dedicated server in five minutes, running it as a service, and a community on your own hostname with the [domain setup guide](docs/hosting-domain.md).

[Editor tutorial video](docs/arena-workshop-tutorial.mp4) · [Logic gates video](docs/logic-gates-tutorial.mp4) · [Logic gates tutorial](docs/logic-gates-tutorial.md) · [Editor plugin API and full guide](docs/editor-plugins.md)

The logic gates walkthrough builds a vault with the gates, watchers and sequences added in 0.16.
