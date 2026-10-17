# Checks

> # what to account for as a maintainer.

Checked on 2026-10-16 with Rust 1.99.0 on Linux: 240 Rust tests and 8 installer tests pass, including UDP and WebSocket integration tests. Formatting, Clippy with warnings denied, the workspace release build, all 92 tank classes, and both bundled mods also pass.

Graphical checks under Xvfb cover the HUD, menu, credits, mode switching, long player names, and the Sandbox button at 800×600, 1280×800, and 1920×1080, with interface scales from 80% to 140%. Captures are in `artifacts/ui-review`. The latest server work and 64/128-client load results are in [server measurements](server-performance.md).

New checks cover Random Events, all ten Co-op PvE waves, squad wipes and revives, enemy pursuit, parry ownership and homing, spawn protection and prediction, Sandbox level shortcuts, and multiplayer mode changes. New GUI captures are in `artifacts/new-modes-review`. The menu and controls fit at 800×600, 1280×800, and 1920×1080. Focused captures confirm short ability and fire taps register, the parry window and cooldown display correctly, and the co-op board shows the squad.

A 15-second release workload with 64 clients and 24 bots kept all clients ready, with no stale clients, skipped snapshots, or dropped catch-up ticks; reported transport loss was 0.94%.

The v0.2.7 menu checks cover Play joining tankyard.kokonico.me, Offline beside it, saved custom addresses, and cancelling a stalled WebSocket handshake. Captures are in `artifacts/central-play-review` at 800×600, 1280×800, and 1920×1080. The Linux release archive runs and imports no glibc symbol newer than 2.30. Actionlint, ShellCheck, and installer fixture tests pass. The tagged workflow builds all platform packages and tests the published installers on Linux, macOS, and Windows.

## Previous runs

The notes below predate the current development checks.

**New UI / server release-candidate changes:** The fixes after 0.2.6 have not been compiled or played/tested in this environment (Cargo and graphical runtime were unavailable here). The earlier test counts and benchmarks below describe older builds, **not** this candidate. Follow [the pre-release checklist](release-checklist.md) on a machine with Rust, graphics, and UDP networking before tagging a release.


# Below is if i released before but i didnt so ignore it, but take it for account for future stuff (for maintainers)

Version 0.2.5 is checked on Linux with Rust 1.99.0. Use these commands after editing:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --release --workspace
cargo run --locked -p tankyard-tools -- validate
```

All 199 headless tests pass, along with formatting, Clippy, documentation checks, and integration test compilation. Checks cover UTF-8 text editing, filtering function-key glyphs, held-key repeat, practice loadouts and records, result freezing, separation from normal achievements, menu history, and field focus. Bot tests check motion learning, confidence after reversals, memory limits, experience across respawns, and blocked sight lines.

Drone checks cover a full 16-drone swarm starting at one position, packing at the cursor, distant commands, recall, wall sliding, arena corners, mixed sizes, fast crossings, projectile storage order, and enemy damage. Up to 128 allied drones can share a cursor while keeping separate bodies. Wall checks follow long, thin, moved, and removed walls. Drones with dead owners expire before another hit. Visual heading checks cover resting drones, frame rates from 30 to 144 Hz, and reduced motion. The dedicated stress command is `tankyard-tools drone-bench --swarms 64 --ticks 1200`.

Model checks cover mount symmetry across all 91 classes and front/rear alignment at several headings. Firing tests cover Twin, Gunner, Triplet, Triple Twin, Streamliner, and Double Burst at zero and maximum reload, including staggered shots, occasional volleys, stable average rate, and burst spacing. Seeded traces are reproducible. Prediction and the server share reload timing; the recoil agreement test includes Double Burst. Sustained direct damage stays within the existing tier budgets over a minute of steady fire after warmup. Full model sheets before and after alignment are in `artifacts/models`; the focused comparison is `before-alignment.png` and `after-alignment.png`.

The suite also covers combat, respawning, skill limits, drone control, movement prediction, tank geometry, recoil, routing, team objectives, snapshot reuse, mod hooks, and moderation. Multiplayer integration tests compile, including match administration, round restarts, account handoff, and modded catalog transfer. All 91 tank classes and both bundled mods validate. The model exporter renders the game's geometry without a window; images are in `artifacts/models`.

The 0.2.2 release benchmarks use 64 simulated players, 24 bots, roughly 3,000 replenished projectiles, a 120-tick warmup, and 600 measured ticks on an i7-9700 with seed 42:

| Arena | Previous average tick | Current average tick | Current p99 tick | Previous/current average AI |
| --- | --- | --- | --- | --- |
| Open | 0.821 ms | 0.817 ms | 1.057 ms | 0.074 / 0.075 ms |
| Maze | 28.207 ms | 6.962 ms | 8.612 ms | 1.205 / 0.395 ms |

Cheap swept bounds checks avoid detailed collision work for distant walls. The Maze simulation average fell by 75.3%; Open performance remained similar. Open snapshots average 0.142 ms and 23.4 allocation calls per viewer, retaining the previous buffer reuse improvement. Reports are `artifacts/bench/before-ai-admin-{open,maze}.json` and `artifacts/bench/after-ai-admin-{open,maze}.json`. These measurements cover simulation and serialization, not rendering or internet latency.

The 0.2.4 learning changes were measured against the preceding compatible build using the same scenario and CPU:

| Arena | Previous average tick | 0.2.4 average tick | 0.2.4 p99 tick | Previous / current average AI |
| --- | --- | --- | --- | --- |
| Open | 0.818 ms | 0.837 ms | 0.934 ms | 0.076 / 0.076 ms |
| Maze | 6.767 ms | 6.679 ms | 8.415 ms | 0.384 / 0.410 ms |

The added learning keeps simulation time close to the previous build in these samples. It does not establish a rendering or network latency improvement. Reports are `artifacts/bench/{before,after}-adaptive-{open,maze}.json`.

After the model and firing changes, the same crowded scenario measured 0.825 ms average / 0.874 ms p99 in Open, and 6.568 ms average / 9.569 ms p99 in Maze. Reports are `artifacts/bench/final-0.2.4-{open,maze}.json`. A five-minute, 24-bot arena also completed with farming, upgrades, combat, and respawns; its final state is in `artifacts/checks/adaptive-bot-arena.csv`.

The 0.2.5 drone changes use the same mixed stress scenario and CPU:

| Arena | 0.2.4 average / p99 tick | 0.2.5 average / p99 tick |
| --- | --- | --- |
| Open | 0.831 / 1.067 ms | 0.944 / 1.187 ms |
| Maze | 6.611 / 9.582 ms | 6.506 / 8.399 ms |

Physical drone contacts add about 0.11 ms in the Open sample. Indexing nearby walls keeps the Maze sample close to its previous cost. Reports are `artifacts/bench/{before,after}-drone-{open,maze}.json`. The dedicated test with 64 independent swarms, 1,024 drones, cursor changes, and recall averaged 0.865 ms, with a 1.440 ms p99, over 1,200 measured ticks after 120 warmup ticks. Bodies stayed separate; maximum compression into the outline margin was 0.181 world units. Its report is `artifacts/bench/drone-swarms-0.2.5.json`. These are headless simulation measurements, without rendering or network traffic.

This environment prohibits UDP sockets and X11 connections. The match administration integration test stopped at socket setup with `Operation not permitted`. Automated window tests cannot run here. On the user's Solus/NVIDIA desktop, normal and safe graphics startup worked; the first 0.2.4 fixes also passed arrow-key editing, held Backspace, and closing. The user subsequently confirmed that Practice opens with F6, F1 opens controls, and the new pause-menu entries work. The closing crash was traced to a UI font destructor running after OpenGL unloaded; graphics resources now have an explicit lifetime inside the window's lifetime.

Extended practice and desktop play, Mint runtime behavior, multiplayer runtime behavior, and render frame rates still need a normal desktop check. See [Linux builds](linux-builds.md) for the single-file client and startup diagnostics. Run `scripts/smoke_client.sh --all` and the UDP load command in the [hosting guide](hosting.md) on a normal desktop. Windows and macOS runtime checks also remain pending. Logs are in `artifacts/checks`.

To reproduce the headless checks in this environment:

```sh
cargo test --locked --workspace --lib --bins -- --skip netem::tests::forwards_actual_udp_and_applies_delay
cargo test --locked --workspace --doc
cargo test --locked --workspace --no-run
```

Accounts, matchmaking, a public server directory, and cloud provisioning are not deployed by this repository. The account and global moderation adapters require a trusted backend implementation before account-based operation. Native JSON packs run through bounded hooks; compiled Rust extensions remain trusted process code.
