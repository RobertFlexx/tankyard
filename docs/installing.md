# Installing Tank Yard

Linux and macOS:

~~~sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh
~~~

The installer downloads a precompiled build, checks its SHA-256 checksum, installs it for your user, and adds an application shortcut. Run the same command to update. Your settings and progress stay where they are.

Windows, from PowerShell:

~~~powershell
irm https://github.com/RobertFlexx/tankyard/releases/latest/download/install.ps1 | iex
~~~

Or download the Windows ZIP from [Releases](https://github.com/RobertFlexx/tankyard/releases/latest), extract it, and open **tankyard.exe**. The PowerShell installer adds a Start menu shortcut and puts Tank Yard on your user PATH.

## Options

Download the script first to inspect it or choose an install location:

~~~sh
curl -fsSLO https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh
sh install.sh --bin-dir "$HOME/.local/bin"
sh install.sh --version v0.2.7 --no-desktop
sh install.sh --server
~~~

Use --server to install the dedicated server and WebSocket relay. Copy the sample server configuration out of the release archive, then edit it before hosting. Updates do not replace your server configuration.

PowerShell accepts -Version, -InstallDir, -NoShortcut, -NoPath, and -Server.

Linux builds support x86-64 and ARM64 with glibc 2.31 or newer. The game uses your system's X11 and OpenGL libraries. macOS has a universal build for Intel and Apple Silicon, including a **Tank Yard.app** bundle; the installer places it in your Applications folder. Windows has an x86-64 build with the C runtime included.

## Playing

**Play** or Enter connects to **tankyard.kokonico.me**. **Play offline** sits beside it. Choose Offline modes to change the local mode, map, and bots. Join server still accepts custom addresses, and keeps them separate from the main Play button.

## Release builds

Pushing a tag matching Cargo's version, such as v0.2.7, runs the Release workflow. It checks the source and installers, builds all four packages, generates SHA256SUMS, and publishes the release after every package is ready. Each archive includes the game, server, relay, command-line tools, example configs, bundled mods, and licenses.

The workflow then installs the published game into a temporary directory on Linux, macOS, and Windows. This checks the same downloads and installer paths players use. A failed build leaves the release unpublished. To retry an interrupted run, use Re-run jobs or dispatch Release with the existing tag. Use a new version tag to change an already published release.
