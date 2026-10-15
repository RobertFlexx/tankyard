# Pre-release checks

This is a checklist, not a statement that the release has passed. The recent UI and networking changes still need a Rust build and an actual graphical/multiplayer test on the target machines.

## Build and source

Run these from the repository root:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release --workspace
cargo run --locked -p tankyard-tools -- validate
```

If the machine allows UDP sockets, run the network integration tests rather than relying only on the in-process unit suite. Check release binaries on Solus/FreeBSD/macOS/Windows individually where supported; a Linux-only build doesn't prove the others.

## The visual stuff

Check at 800×600, 1024×768, 1280×800, 1600×900, 1920×1080, ultrawide, and with UI scale at 80%, 100%, and 140%:

- Display: Shape Quality, AA, FPS cap, and Texture Filtering must never overlap each other or the footer. Dark Theme belongs in Style, not on top of Shape Quality.
- Interface Scale at its default value reads **100%**, not 33%. Frame limit highlights its selected option. An unsupported anisotropy option must remain disabled.
- Cycle through Display/Style/Controls/Accessibility/Profile, then Back and into every other menu. There should be no ghost text, retained selection overlay, or clickable invisible old control.
- Test long server addresses, Unicode player names, multiple favorites, and long config paths. Labels must stay inside buttons; wrapping must not spill into neighboring columns.
- Toggle fullscreen, change window size, save, restart, and verify the chosen settings survive. Check both light and dark theme, reduced motion, and colorblind/high-contrast settings.
- Type, select, paste and delete text, click outside a field, rebind keys and hit Escape. The UI must not trigger unrelated shortcuts while capturing input.

## Gameplay and multiplayer

- Factory/Fabricator: six-mini-tank cap, minion respawn after one is destroyed, cursor orbit, right-click recall, independent minion shooting, bullets blocked by walls, correct ownership/damage scoring.
- Other drone classes: cap and movement independent of Factory, no drone/projectile duplication on evolution, server and HUD showing consistent deployed counts.
- Mixed projectile classes: stats are context-aware, and upgrade point spending actually changes the behavior described by its tooltip.
- Tank progression: buy points, evolve, die/respawn, keep/depreciate scores correctly, reset on a new round, and rejoin mid-match. Check bots around obstacles and against protected/friendly targets.
- Network: packet loss and latency, high event bursts, join/leave spam, malformed input, unauthorized sandbox/admin requests, disconnect during a kill, and slow clients. Confirm important events arrive in order or slow clients are explicitly disconnected, not silently lost.
- Operate the dedicated server with authenticating netcode tokens before public internet hosting. The default local/community host mode is **not authenticated transport**; a password is not a substitute for secure tokens.
- Load a corrupt/oversized bans file and server config: startup must reject them without allocating an arbitrarily large read buffer.
- Confirm official/community chat permissions, mutes, bans, account revocation, and the host operator console still work.

Release only after the actual desktop screenshot/regression checks, Cargo checks, and network abuse tests pass. Fix the first failing check rather than checking a box based on a previous version.
