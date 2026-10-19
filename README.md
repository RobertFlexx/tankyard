# Tank Yard

[![Release](https://img.shields.io/github/v/release/RobertFlexx/tankyard?style=flat-square)](https://github.com/RobertFlexx/tankyard/releases/latest)
[![Build](https://github.com/RobertFlexx/tankyard/actions/workflows/release.yml/badge.svg)](https://github.com/RobertFlexx/tankyard/actions/workflows/release.yml)
[![License: MIT or Apache-2.0](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-64748b?style=flat-square)](LICENSE)

A tank arena by RobertFlexx, inspired by Diep.io and Arras.io. Play online, host a game, or practice offline.

**[Download](https://github.com/RobertFlexx/tankyard/releases/latest)** · [Installation](docs/installing.md) · [How to play](docs/playing.md) · [Hosting](docs/hosting.md) · [Modding](docs/modding.md)

Choose **Play** for the public server or **Play offline** for a local game.

Windows (64-bit): [NSIS installer (.exe)](https://github.com/RobertFlexx/tankyard/releases/download/v0.3.3/tankyard-v0.3.3-windows-x86_64-setup.exe) · [Portable ZIP](https://github.com/RobertFlexx/tankyard/releases/download/v0.3.3/tankyard-v0.3.3-windows-x86_64.zip). For the portable version, extract the ZIP and open `tankyard.exe`.

Install or update on Linux and macOS:

```sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
```

Build from source with Rust 1.88 or newer:

```sh
cargo build --locked --release --workspace
./target/release/tankyard
```

See [Linux build requirements](docs/linux-builds.md) and [third-party notices](NOTICE.md).

## Tank you!

Tank Yard has been years in the making. Thank you to everyone who’s played, shared ideas, and supported it.
