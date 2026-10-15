# Tank Yard

Tank Yard is a native tank arena inspired by Diep.io and Arras.io. Shoot shapes, earn skill points, upgrade your tank, and fight for score. It has 91 tank classes, twelve modes, four practice challenges with personal records, local achievements, and player-hosted multiplayer.

Build with Rust 1.88 or newer:

```sh
cargo build --locked --release --workspace
./target/release/tankyard --offline
```

Linux needs a C linker and X11/OpenGL. On Debian or Ubuntu, install `build-essential libx11-dev libxi-dev libgl1-mesa-dev`.

For a Linux binary to share across distributions, use `scripts/build_linux_compatible.sh` with Zig installed. It targets glibc 2.31 and baseline x86-64 CPUs. The client uses the desktop's shared glibc and graphics drivers; static glibc causes a reproduced NVIDIA startup crash. See [Linux builds and startup diagnostics](docs/linux-builds.md).

Use WASD to move, the mouse to aim, left click to fire, E for auto-fire, Space for your ability, and 1–8 for skills. Tab opens tank upgrades; Esc pauses. After death, press Enter or click Respawn. Controls can be rebound in Settings.

Hold Shift with a skill key or click to fill that stat. While firing, drones follow the cursor; right click recalls them. Friendly drones bunch together with physical contact, slide along walls, and keep their individual shapes and headings when resting.

The Factory is the fun oddball: it builds up to six tiny tanks with their own cannons. Hold left click and they'll circle your cursor while firing. Let go and they'll pick fights with nearby visible targets, then hang back around your tank when it's quiet. Kill a minion and the launcher will replace it when reloaded. The Assembly upgrade keeps the same six-unit cap.

Skill labels follow your class: an Overseer gets Drone Speed / Health / Damage, the Factory gets Minion stats, and the Trapper gets trap stats. Mixed tanks spell out which bits benefit in the hover descriptions. The underlying upgrade values have not changed.

Practice is beside Play on the main menu, in the pause menu, and on F6. It offers a shape sprint, drone drill, tank skirmish, and survival challenge with personal records. Press F1 for the current controls, F3 for diagnostics, F10 for settings, and F5 to restart an offline game from pause. Tab and Shift+Tab move between text fields; Esc goes back. See [playing](docs/playing.md) for goals and shortcuts.

Game modes lets you choose Beginner through Expert bots; your choice also applies when hosting. Bots estimate visible opponents' movement, adjust their aim as they learn the pattern, and change their spacing after damage, kills, and deaths. This experience lasts for the match.

Multi-barrel guns use independent firing phases, producing staggered shots and occasional volleys. Double Burst and Overtrapper have centered front and rear mounts; Necromancer has two opposing drone ports. Rendering and simulation use the same mount positions.

Text fields support arrow keys, Home/End, Shift selection, Ctrl+A/C/X/V, and held Backspace/Delete. Ctrl with an arrow or deletion key moves or deletes by word.

Use Host server or Join server in the menu for multiplayer. Community servers support chat rooms and scoped admin permissions. Regular host/join uses unauthenticated netcode; it is for local/private games, not secure public hosting. Public deployments need authenticated tokens and a trusted token issuer (not bundled). Official quick play has no chat. To run a dedicated server:

```sh
./target/release/tankyard-server --config configs/server.toml
```

The Mods menu enables local packs. Servers can load custom tanks, maps, rules, textures, and behavior hooks. Account and cloud services have backend interfaces; those services are not included.

See [hosting](docs/hosting.md), [modding](docs/modding.md), [Linux builds](docs/linux-builds.md), [checks](docs/status.md), and the [release checklist](docs/release-checklist.md).

Source is licensed under MIT or Apache-2.0. Font and dependency licenses are in [NOTICE](NOTICE.md). Tank Yard is independent and is not affiliated with Diep.io or Arras.io.
