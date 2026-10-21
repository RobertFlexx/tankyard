# v0.6.0 checks (2026-10-20)

421 public Rust tests, five real-key private authorization tests and ten installer fixtures pass. Strict Clippy, formatting, all 103 tank profiles, both bundled mods and eight maps pass validation. New coverage includes authenticated credential envelopes, wrong passwords and identities, tampering/randomness, Infection timing/conversion/late joins/disconnects, party survivor preservation, occupied/contested moving-hill scoring, all-map hill clearance, mode-aware bot goals, longer protocol timer limits and lethal host changes.

Native tests cover clipboard TARGETS discovery and cross-process Unicode exchange, party invite copying and text-box paste/copy at 1280×800 and 800×600 with 140% UI, the sixteen-mode grid, and both new modes in actual gameplay. Both relocated/renamed private editions remain locked even with a matching profile key and direct-join/smoke options: a bad password starts no connection; pasting the correct password permits it. Captures and logs are in `artifacts/v060-review`. Generated passwords remain separate from the private archives; no password or plaintext signing seed is embedded in those binaries.

The tagged workflow builds all public platforms, checks native clipboard interoperability and validates downloaded installers. Protocol 18 requires upgrading clients, the central Pi's regular server and its relay together. Publishing does not deploy that process. [Security review](security-review-v060.md) and [operator steps](central-pi-upgrade.md) describe the scope and remaining limits.

# v0.5.0 checks (2026-10-20)

411 public Rust tests and ten installer fixtures pass, with formatting and strict Clippy. New tests exercise party proof binding, admission failure, allied replacement and refill, preserved party teams across round changes, independent wave enemies, empty-server rotation deadlines, complete tree connections and creator event authorization/cooldowns/revocation, wall-safe Axiom dashes, working sentries, limited healing and hostile-only gravity. Native GUI fixtures at 1280×800 and 800×600 with 140% UI cover the connected tree, party creation/host badge, creator Showtime, transformation/attacks/reset and the uptime countdown.

Five additional private real-key integration tests pass against ordinary local servers, including Axiom signatures, player spawn protection, cooldown rejection and immediate revocation cleanup.

Protocol is now 17. Upgrade regular clients, servers and relays together using the [Pi instructions](central-pi-upgrade.md); publishing does not deploy the central server. [Party setup](parties.md) explains leader-hosted IP connectivity. The [scoped review](security-review-v050.md) records authority and transport limits. Private credential editions and keys remain local.

# v0.4.1 checks (2026-10-20)

398 public Rust tests, ten installer fixtures, formatting, Clippy with warnings denied, all 103 tank profiles and both bundled mods pass on Linux. New regressions cover preference reset without profile loss, bounded custom-map validation work, procedural cover containment/nonoverlap, Maze connectivity at small and maximum dimensions, scaling without coordinate drift, safe map-switch rollback, quicker population growth and nearby collision recovery in large maps.

Native clicks at 800×600 and 1280×800 with 140% UI scale verify all seven Settings tabs, applying/persisting defaults while retaining profile progress and saved servers, and the bottom-right owner menu with a full leaderboard. A public client with the creator key successfully submits an audited Maps-tab change to an ordinary central-profile server. Native screenshots and a map layout/artwork sheet are in `artifacts/v041-review`.

Protocol remains 16. Update the regular server for the new layouts, dynamic cover and signed creator map command. The [scoped review](security-review-v041.md) records the validation-work limit and other fixes. The central Pi still requires its operator's deployment; `server_id = "tankyard-central"` is required for central-owner access.

# v0.4.0 checks (2026-10-20)

389 public Rust tests, ten installer tests, formatting and Clippy with warnings denied pass on Linux. Four private real-key network tests verify creator and central-owner identity, host policy, impostors, replay/tampering rejection, revocation/re-enable, signed creator powers and a central-server-only ban. The compatible Linux build and ABI checks cover all five public executables. No identity signing credential is included in public source or packages.

Native checks at 1280×800 and 800×600 with 140% interface scale and 100%/200% desktop DPI verify mouse skill upgrades while evolution choices remain open, automatic local Host assignment/badges, all five staff tabs, responsive centered panels, warning/announcement delivery and the central-owner meeting achievement. Ordinary public clients work with adjacent and profile key files; both self-contained credential editions work after relocation/renaming with no key sidecars and an unrelated working directory. Screenshots and logs are in `artifacts/v040-review`.

Eight release UDP clients under 200 ms simulated latency, 75 ms jitter and 10% packet loss finish ready with zero stale clients, skipped snapshots or dropped catch-up ticks. Reliable bursts complete without disconnecting players, and failed ban storage leaves no hidden sanction. RustSec reports zero vulnerability entries, with outstanding Macroquad soundness and fxhash/ttf-parser maintenance advisories documented in the [security review](security-review-v040.md). These graphics dependencies are absent from the dedicated server's tree.

The tagged workflow builds all public desktop packages and checks downloaded installers on Linux, macOS and Windows. Protocol 16 requires updating the central Pi's server and relay; [operator steps](central-pi-upgrade.md) are provided. No live deployment was performed.

# v0.3.3 checks (2026-10-19)

378 public Rust tests, ten installer tests, formatting, and Clippy with warnings denied pass on Linux. The compatible release build passes ABI checks for all five public executables, with no glibc imports newer than 2.30. Three private real-key integration tests pass against the regular server, including host consent, impersonation/replay rejection, revocation, and Bastion's treatment of signed creator powers.

Native GUI checks at 800×600 and 1280×800, including 200% desktop DPI, verify saved filter preferences without a texture pack, a steady layout during scale drags, the exact release position, cancelled edits when navigating away, and readable settings errors above the footer. A loaded checker texture verifies all five filter levels from 1× through 16×, with mipmaps generated exactly once. The moved private executable authenticates and sends signed commands after two relocations and renames, from an unrelated working directory, with no adjacent key file and a nonexistent key environment path. Its embedded credential and private source remain excluded from GitHub and public packages. Captures and logs are in `artifacts/v033-review`. Cross-platform builds and installed-package checks run in the tagged release workflow; publishing does not deploy the central Pi/Docker process.

# v0.3.2 checks (2026-10-19)

374 public Rust tests and ten installer tests pass on Linux, with formatting, Clippy with warnings denied, all 103 tank profiles, both bundled mods, and the compatible Linux release build. Linux executables import no glibc newer than 2.30. Three private real-key integration tests verify the ordinary official server, impersonation/tampering rejection, revocation, and signed god mode, 6× movement/reload, 8× damage, unlimited abilities, and teleporting during real network input. The private client and credentials remain excluded from Git and public packages.

Native GUI checks at 800×600 and 1280×800 verify visible colorblind/high-contrast changes, 80%–140% interface scaling, fixed navigation while scrolling, audio controls, Bastion Credits, and enlarged upgrade/Sandbox panels alongside the boss banner and other HUD regions. Reduced-motion menu captures are pixel-identical across time. Two-monitor sessions at 100% and 200% desktop DPI verify Native window sizing, fullscreen placement, a 1024×768 render target with 4× MSAA, restored window dimensions, scaled cursor, continuous sliders, and interface-scale dragging. Captures are in `artifacts/v032-settings`, `artifacts/v032-settings-small`, `artifacts/v032-display-review`, `artifacts/v032-hud-review`, and `artifacts/v032-resolution-matrix`. A further native menu sweep covers 800×600, 1024×768, 1280×800, 1920×1080, 2560×1440, 3440×1440, portrait 800×1200, and 3840×2160 at 80%, 100%, and 140% interface scale.

Eight real UDP clients through simulated 200 ms latency, 75 ms jitter, and 10% packet loss all finish ready with zero stale viewers. Bastion has no ping-based suspicion or automatic cheating bans. New tests cover sequence wrap/reordering/resume without extra simulation time, bounded intentions, foreign signing keys, palette separation, motion suppression, and layout bounds. Windows/macOS binaries and installed-package checks run in the tagged workflow; they were not run interactively on this Linux workstation. Publishing does not deploy the central Pi/Docker process.

# v0.3.1 checks (2026-10-19)

366 public Rust tests and ten installer tests pass on Linux. Formatting, Clippy with warnings denied, all 103 tank profiles, both bundled mods, the compatible Linux workspace release build, and ABI checks pass. Public/private Linux clients require no glibc newer than 2.30. Native Windows and macOS window-control calls separately compile-check for their targets; full desktop builds and installed-package checks run in the tagged release workflow.

A virtual two-monitor X11 session at 100% and 200% desktop DPI verifies remembered secondary-monitor placement, Native window fitting, fullscreen staying on that display, switching fullscreen render resolution, exact restored window dimensions, continuous slider positioning, fast clicks, and capture while interface scale reflows. Cursor images measure 32×32 and 64×64 respectively, replacing the tiny bitmap path. Captures and logs are in `artifacts/v031-review`.

Creator policy tests cover official defaults, explicit official revocation, community/default opt-out, password-protected arenas, signed requests, identity impersonation, replay/tampering/round/rate checks, audit failure, and containment of trusted extension faults. Relay checks cover shared-address bans with bystanders/new connections, legacy bans, absolute upgrade deadlines, invalid limits, packet budgets, oversized/empty UDP drops, backpressure, and bidirectional forwarding. The private client and signing key remain excluded from public source and release assets. The Pi/Docker process requires an operator upgrade; publishing a release does not deploy it.

# v0.3.0 checks (2026-10-18)

356 public Rust tests and ten installer tests pass on Linux. Formatting, Clippy with warnings denied, workflow validation, all 103 tank profiles, both bundled mods, and the compatible Linux workspace release build pass. Public and private Linux client binaries import no glibc newer than 2.30. Additional private integration checks use the real signing key against the ordinary server: official opt-in, community/default opt-out, an impersonator, tampered requests, moderation, revocation/re-enable, invalidated old authorization, and round identity continuity. The private signing key and creator client source remain outside Git and public packages.

Native captures in `artifacts/v030-review` cover Join server, all 32 favorites, paging, sorting, reordering, per-entry deletion, independent history/favorites, starring, immediate persistence, Settings → Servers, and 800×600 light/dark layouts. A private-client/ordinary-server/public-client session verifies the creator badge, saved meeting achievement, signed gameplay commands, a boss arrival, moderation, and immediate operator revocation. Public signature fixtures contain only signatures over a fixed test nonce, with no signing credential. Authorization tests include valid-but-nonconsenting hosts, replay, payload/server/nonce/round binding, rate limits, invalid values, audit failures, and containment/auditing of trusted extension faults.

Boss checks cover the Summoner's shorter acquisition and owner leash, protected/low-level players and cover, capped swarms, and nonoverlapping warning/special/primary timing. The balance changes retain the previous larger health reserves and rare aggregate encounter chance. Protocol 15 requires updating clients, servers, and relays together. The central Raspberry Pi/Docker configuration and update steps are included; publishing does not update the live deployment. Other desktop platforms and the graphical Windows installer are built and checked by the tagged release workflow, not run interactively on this Linux workstation.

# v0.2.16 checks (2026-10-18)

336 public Rust tests and ten installer tests pass on Linux, including real UDP/WebSocket integration tests. Formatting, Clippy with warnings denied, workflow validation, all 103 tank profiles, both bundled mods, and the compatible Linux release build pass. New coverage includes Smasher contact counters and recoil, stationary fifteen-second spawn immunity under auto-fire/ability inputs, stasis anchoring and release, caster death/removal, cover and allied rounds, boss secondary attack windups, creator impersonation rejection, and the meeting achievement. The source tree includes verification and optional extension interfaces; private owner tools and signing credentials are excluded from Git and release packages.

Native GUI captures in `artifacts/v0216-review` cover the new combat-trial page, offline setup and its 800×600 layout, stationary aiming/immunity, the Chronarch field and wave, creator badges/panel, and a centered boss arrival. Boss banners, practice objectives, kill notices, general notices, and achievement popups reserve separate positions in a centered vertical stack; foreground panels defer achievement toasts. Tank and boss model sheets use the game's actual vector geometry. A native private-server/public-client pairing verifies the creator badge, saved meeting achievement, signed speed changes, and boss alerts. Separate private checks cover key-based authentication, tampered commands, impersonation, round identity persistence, invulnerability, and unlimited abilities. The NSIS graphical installer compiles with the real compiler on Linux; Windows CI tests real silent installation, update, configuration preservation, and uninstall before publishing. Public and private Linux binaries import no glibc newer than 2.30. Other desktop platforms are built and checked by the tagged workflow; they were not run interactively on this Linux workstation.

Protocol 14 requires updating clients and servers together. Release checks publish four platform archives plus the graphical Windows setup after all required jobs pass.

# v0.2.15 checks (2026-10-18)

323 Rust tests and eight installer tests pass on Linux, including real UDP/WebSocket integration tests. Formatting, Clippy with warnings denied, workflow validation, all 102 class profiles, both bundled mods, and the compatible Linux workspace release build pass. Coverage includes all eleven bosses, aggregate spawn scheduling, capped swarms, arrival grace, adaptive borders, wall pinning, deployed drone upgrades, neural tactical choices, friendship/betrayal, stable parry mistakes, Opus decoding/loss recovery, and alert expiry.

Native GUI checks use virtual audio devices and a 1920×1080 virtual display. Captures in `artifacts/boss-review` and `artifacts/ui-polish-review` cover alarm/voice playback, boss arrivals and banner expiry, smooth arena resizing, monitor resolution syncing, Sandbox level slider dragging outside the track, selected levels, open chat with Sandbox, both themes, and small/large display layouts. Layout tests cover 800×600 through 3440×1440 and a portrait viewport at 80%–140% interface scale. Full-band voice hardware/headset conversations and non-Linux monitor behavior remain manual platform checks.

The compatible release imports glibc 2.30; its codec is statically bundled. A synthetic release workload with 64 participants, 24 bots, about 3000 projectiles, and 932 shapes measured simulation p99 1.21 ms, AI p99 0.12 ms, and snapshot p99 0.23 ms on an i7-9700. These are local headless measurements, not internet latency or rendering guarantees. The previous network-emulator test's send-counter race is fixed by joining the relay before checking final statistics.

Clients and servers must update together for protocol 13. The tagged Release workflow validates source and builds Linux x86-64/ARM64, macOS universal, and Windows x86-64 packages before publishing.

# Checks

**0.2.14 development checks (2026-10-18):** All 309 Rust tests and eight installer tests pass on Linux, including real UDP/WebSocket integration tests. Formatting, Clippy with warnings denied, all 102 classes, both bundled mods, and the compatible Linux workspace release build pass. New coverage includes physical swarm capture/merging, cursor and automatic drone control, tier progression, voice codec/resampling, bounded playback, distance routing, replay rejection, moderator mutes, and spectator interest returning correctly on respawn. Clients and servers must update together for protocol 12.

Native graphical checks cover captured drones, microphone mute/unmute icons, virtual-device voice playback, spectator cycling, respawn, chat while spectating, chat persisting beyond its former timeout, upgrades beside chat, Audio settings, both themes, and 800×600 / 1280×800 / 1920×1080 layouts. Captures are in `artifacts/feature-review`. The release executable imports glibc 2.30 and only ALSA 0.9-era symbols; `scripts/check_linux_abi.py` guards the compatibility limits in release CI. Actual headset conversations and macOS microphone permissions remain manual platform checks. The tagged Release workflow builds and checks binaries for Linux, Windows, and macOS before publication.


> # what to account for as a maintainer.

Checked on 2026-10-17 with Rust 1.99.0 on Linux: 289 Rust tests and 8 installer tests pass, including UDP and WebSocket integration tests. Formatting, Clippy with warnings denied, the compatible Linux workspace release build, all 102 tank classes, and both bundled mods also pass.

Graphical checks under Xvfb cover the HUD, menu, credits, mode switching, long player names, and the Sandbox button at 800×600, 1280×800, and 1920×1080, with interface scales from 80% to 140%. Captures are in `artifacts/ui-review`. The latest server work and 64/128-client load results are in [server measurements](server-performance.md).

New checks cover Random Events, all ten Co-op PvE waves, squad wipes and revives, enemy pursuit, parry ownership and homing, spawn protection and prediction, Sandbox level shortcuts, and multiplayer mode changes. New GUI captures are in `artifacts/new-modes-review`. The menu and controls fit at 800×600, 1280×800, and 1920×1080. Focused captures confirm short ability and fire taps register, the parry window and cooldown display correctly, and the co-op board shows the squad.

A 15-second release workload with 64 clients and 24 bots kept all clients ready, with no stale clients, skipped snapshots, or dropped catch-up ticks; reported transport loss was 0.94%.

The v0.2.8 menu checks cover Play joining tankyard.kokonico.me, Offline beside it, saved custom addresses, and cancelling a stalled WebSocket handshake. Captures are in `artifacts/central-play-review` at 800×600, 1280×800, and 1920×1080. The Linux release archive runs and imports no glibc symbol newer than 2.30. Actionlint, ShellCheck, and installer fixture tests pass. The tagged workflow builds all platform packages and tests the published installers on Linux, macOS, and Windows.

The v0.2.9 checks cover empty-arena sleep and resume, adaptive bot counts through joins/leaves and round restarts, delayed input bursts without kicks, short taps, per-match/global/off achievements, neural prediction training and limits, and WebSocket backpressure and early disconnect reporting. Current load measurements are in [server performance](server-performance.md#v029-follow-up). Native icon assets are bundled for all desktop platforms; the tagged workflow tests Windows installation against its archive before publication.

GUI checks at 800×600, 1280×800, and 1920×1080 cover all three achievement modes, saved preferences, protected and moving tanks, native window icon/class properties, and a measured white pulse while idle. A local WebSocket relay was stopped and restarted to verify the disconnect screen and successful Reconnect. Captures are in `artifacts/v029-review`.

The v0.2.10 homing checks cover bounded turning without slowing into a hover, forward-only acquisition and retargeting, a successful aimed hit, a successful late sidestep, flight expiry for all 16 homing barrels at base and upgraded speeds, the final tick's travel limit, and repeated parries without lifetime refresh.

The v0.2.11 checks cover the revised XP and skill schedule through death and respawn, skill-panel visibility, sight limits at every bot difficulty, automatic mount limits, withdrawing from guided threats, and blocked or friendly threats. Build-learning tests verify that training changes actual evolution and skill commands and that idle time does not train the models. Tests also cover learning visible human class results, ignoring hidden gaps and private skill allocations, personal grudges without hidden targeting, recognizing the real killer through respawn, and Auto Smasher body tactics and shot reach. Memory remains bounded. A five-minute, 24-bot arena completes with combat, evolving builds, and respawns. Its report is `artifacts/checks/v0211-bot-arena.csv`.

The compatible Linux release binary renders 120 offline frames and exits normally under Xvfb. A GUI check confirms level two has no skill point, PgUp to level three awards one, key 1 spends it and hides the panel, and PgUp to level four awards the next point. All five packaged executables report 0.2.11; all 92 classes and both bundled mods validate. The tagged workflow also renders the native Linux x86-64 and ARM64 release builds before publishing. Windows and macOS retain native startup and installer checks.

The v0.2.12 checks cover level 55 and 38 skill points, the complete final-tier upgrade graph, all three parry rewards and missed windows, unchanged ordinary cannon damage, echo pool limits, and trap launch distance. Reload tests follow actual cooldown and charge, including local prediction before a server snapshot arrives and refusing predicted recoil when trap or turret pools are full. Minion tests cover crossing targets, cover, flight range, a cursor on the minion body, settled formations, recall, and passive replacement without recoil. AI tests cover Smasher flanking, close pursuit and retreat, plus parry timing and hidden or expired shots. Network tests recover two missing input frames across sequence wrap without repeating attacks, reject malformed histories, and keep overloaded queues bounded without kicks.

The compatible release game runs at 800×600, 1280×800, and 1920×1080 under Xvfb. GUI checks cover both first-run themes, mouse and keyboard selection, saved choice on restart, existing preferences, a fresh install, and an unwritable profile. They also cover the 30-second pulse ending on movement, shooting/reloaded bars, Counterguard evolving to Reprisal at 55, and all five Sandbox level buttons. The ten new models were inspected at multiple headings using the same geometry as the game. Captures and logs are in `artifacts/v0212-review`. Both sample server configs start and sleep with no humans connected. Final 64-client UDP, eight-client impaired-link, and 32-client WebSocket runs kept every client ready with no stale clients, skipped snapshots, or dropped ticks; measurements are in [server performance](server-performance.md#v0212-follow-up). Actionlint, ShellCheck, and all eight installer fixtures pass.

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
