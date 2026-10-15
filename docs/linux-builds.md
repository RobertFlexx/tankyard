# Linux builds and startup diagnostics

The 0.2.5 client keeps Rust dependencies and built-in assets in one executable and uses the desktop's shared glibc, X11, and OpenGL libraries. The compatibility build targets glibc 2.31 and baseline x86-64 CPUs. Its current executables import no glibc symbol newer than 2.30. A user confirmed normal and safe graphics startup on Solus with NVIDIA, then verified text editing and closing with the first 0.2.4 fixes. Mint runtime verification remains pending.

Build the release with Rust, Zig, and the locked Cargo dependencies available:

```sh
scripts/build_linux_compatible.sh
./target/linux-compatible/x86_64-unknown-linux-gnu/release/tankyard --version
./target/linux-compatible/x86_64-unknown-linux-gnu/release/tankyard
```

The linker wrapper selects `x86_64-linux-gnu.2.31` using [Zig's versioned glibc targets](https://github.com/ziglang/zig/blob/master/lib/libc/glibc/README.md). It keeps its caches inside `target`. A native build with the host's current glibc may require newer symbols than the destination distribution provides.

The old static 0.2.2 client crashed inside NVIDIA GLX initialization: a TLS lookup returned an invalid pointer in `libnvidia-gpucomp`. A small test loading `libGLX_nvidia.so.0` reproduced SIGSEGV with static glibc and initialized successfully with shared glibc. The normal host build also imported `GLIBC_2.43`. Both issues are addressed by the compatibility build. The crash records from the older launch paths were distinguished from the replacement using executable paths and version output.

[glibc's maintainers document that static `dlopen` requires matching dynamic glibc](https://sourceware.org/pipermail/glibc-cvs/2021q2/073295.html). The graphical client therefore rejects `crt-static` on Linux GNU targets. Headless server builds can still use static linking. The old static package is superseded; its graphical executable and the previous release launch locations use the compatible client.

The separate crash on closing came from a thread-local UI font being destroyed after OpenGL had unloaded. The client now releases font and mod texture resources while the window is still alive, and saves progress through the same exit path for the window close button and Quit.

For static headless executables, use `scripts/build_linux_static_headless.sh`. It builds `tankyard-server`, `tankyard-load`, and `tankyard-tools` in `target/static/x86_64-unknown-linux-gnu/release`. The 0.2.5 headless package contains stripped static PIE executables with no ELF interpreter or shared-library dependencies. Its glibc ABI note specifies Linux 6.1.0. The graphical game is in the separate compatible package.

For a downloaded single executable:

```sh
chmod +x ./tankyard
./tankyard --version
./tankyard
```

For a graphics startup problem:

```sh
./tankyard --offline --safe-graphics
```

Safe graphics uses a 1280 by 800 window with MSAA and anisotropic filtering disabled. Normal graphics settings remain available. Each graphical launch writes a bounded startup report to the cache directory, normally `~/.cache/tankyard/last-startup.log`; a Rust panic adds its message and backtrace and prints the path in the terminal. Native driver faults still need the system crash dump. The report contains version and window settings, without recording account credentials or the process environment.

`artifacts/checks/driver-runtime-comparison.json` records the driver-loading comparison. `linux-compatible-build.log` and `linux-compatible-build.json` record the build and ELF requirements. Automated full window tests remain blocked by this workspace's display restrictions; the Solus startup result came from the user's desktop.
