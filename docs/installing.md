# Installing Tank Yard

Linux and macOS:

~~~sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
~~~

The installer downloads a precompiled build, checks its SHA-256 checksum, installs it for your user, and adds an application shortcut with the Tank Yard icon. Run the same command to update. Your settings and progress stay where they are.

FreeBSD 15 and newer (amd64): install the sound plugins once as root with `pkg install alsa-plugins`, then run the installer with the base system's `fetch`:

~~~sh
fetch -o - https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
~~~

[Tank Yard on FreeBSD](freebsd.md) covers NVIDIA, sound and hosting.

macOS: download the **macOS installer (.pkg)** from [Releases](https://github.com/RobertFlexx/tankyard/releases/latest) and open it. The installer places **Tank Yard.app** in Applications; the dedicated server, bundled mods and documentation are inside the app. It runs natively on Apple silicon and Intel Macs with macOS 11 or newer. The package is not notarized by Apple yet, so macOS blocks it when it was downloaded in a browser. Either use the fetch script, which downloads it with `curl`, verifies it against the release's `SHA256SUMS`, and opens the normal visual installer without the warning (`--cli` installs from the terminal instead):

~~~sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install-macos.sh | sh
~~~

or verify and unblock a package you already downloaded with `sh install-macos.sh --file path/to/the.pkg`. That checks the file against the official checksum before it clears the download's quarantine flag, so a modified package is never unblocked. You can also try to open the package, then choose **Open Anyway** in **System Settings → Privacy & Security**. (Control-click → Open no longer bypasses this on macOS 15.) Maintainers can remove the warning entirely by [signing and notarizing](macos-signing.md) releases. Install a newer package to update; your settings and mods are kept. The shell installer above still works on macOS too.

Windows: download the **Windows setup (.exe)** from [Releases](https://github.com/RobertFlexx/tankyard/releases/latest), then open it and follow the installer. It installs for your Windows account without administrator access, creates a Start menu shortcut, offers a desktop shortcut, and appears in Windows Installed apps for removal. Run a newer setup to update. Player settings, progress, host configurations, and unrelated files survive updates and removal.

PowerShell is also available:

~~~powershell
irm https://github.com/RobertFlexx/tankyard/releases/latest/download/install.ps1 | iex
~~~

Or download the Windows ZIP from [Releases](https://github.com/RobertFlexx/tankyard/releases/latest), extract it, and open **tankyard.exe**. The PowerShell installer adds a Start menu shortcut and puts Tank Yard on your user PATH.

## Options

If the game stops unexpectedly, run `tankyard --diagnostics-path` to find its local reports without opening a window. `last-crash.log` keeps the most recent Rust panic across later launches; `last-startup.log` describes the latest launch. Reports stay on your machine. Review a report before attaching it to an issue; driver or operating-system faults may need the system crash report too. Try `tankyard --safe-graphics` for graphics startup problems.

Download the script first to inspect it or choose an install location:

~~~sh
curl -fsSLO https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh
sh install.sh --bin-dir "$HOME/.local/bin"
sh install.sh --version v0.2.9 --no-desktop
sh install.sh --server
~~~

The installers retry busy-server answers from GitHub (502, 503, 504, 429 and dropped connections) with growing pauses, resume interrupted downloads, and stop at once on a missing release. When github.com's download links keep failing, the installers fetch the same files through the GitHub API instead; checksums are verified either way. Set `TANKYARD_GITHUB_TOKEN` to a GitHub token to raise the API's rate limit. Set `TANKYARD_DOWNLOAD_ATTEMPTS` to change the number of tries (6 by default). Nothing is replaced until the download is verified, so it is always safe to run the command again.

Use --server to install the dedicated server and WebSocket relay; it prints the next steps. [Hosting](hosting.md#a-dedicated-server) walks through writing a configuration, opening the port and running it as a service. Updates do not replace your server configuration.

PowerShell accepts -Version, -InstallDir, -NoShortcut, -NoPath, and -Server.

Linux x86-64 builds need glibc 2.31 or newer; ARM64 builds need glibc 2.35 or newer. The game uses your system's X11 and OpenGL libraries. FreeBSD builds need FreeBSD 15.0 or newer on amd64, `alsa-plugins`, and an X11 desktop. macOS has a universal build for Intel and Apple Silicon, including a **Tank Yard.app** bundle; the installer places it in your Applications folder. Windows has an x86-64 build with the C runtime included.

## Playing

**Play** or Enter connects to **tankyard.kokonico.me**. **Play offline** sits beside it. Choose Offline modes to change the local mode, map, and bots. Join server still accepts custom addresses, and keeps them separate from the main Play button.

## Release builds

Pushing a tag matching Cargo's version, such as v0.2.8, runs the Release workflow. It checks the source and installers, builds all five platform archives (FreeBSD's in a FreeBSD VM) and the graphical Windows setup, generates SHA256SUMS, and publishes the release after every package is ready. Each archive includes the game, server, relay, command-line tools, example configs, bundled mods, and licenses. Windows CI compiles the NSIS graphical installer and tests silent install, update, and uninstall against the real game before publication.

The workflow then installs the published game into a temporary directory on Linux, macOS, Windows and FreeBSD. This checks the same downloads and installer paths players use. A failed build leaves the release unpublished. To retry an interrupted run, use Re-run jobs or dispatch Release with the existing tag. Use a new version tag to change an already published release.
