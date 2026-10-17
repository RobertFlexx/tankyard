# Modding

Enable a pack in the Mods menu, pass `--mod path` to the client or server, or list directories in the server's `mods` setting. Packs work offline and on community servers. A pack has a `mod.json`; the examples in `mods/vampire` and `mods/fast_arena` run without rebuilding.

```sh
cargo run --locked -p tankyard-tools -- validate-mods --mod mods/vampire --mod mods/fast_arena
./target/release/tankyard --offline --mod mods/vampire
./target/release/tankyard-server --mod mods/fast_arena
```

API version 1 manifests require `api_version`, `id`, `name`, and `version`. Optional fields are `dependencies`, `classes`, `maps`, `rules`, `mode_name`, `textures`, and `hooks`. Dependencies load first; missing dependencies and cycles are rejected. Definitions replace an existing ID or add a new one. Among packs whose dependencies are ready, command-line order decides loading order. Later definitions win. A supplied `rules` object replaces the complete rule profile; omitted fields take standard defaults. Packs without a rules object keep the configured server physics. A custom `mode_name` labels the underlying mode, and hooks provide its extra behavior.

Rules control movement speed, acceleration, coasting, damage, health, regeneration, XP, score, and respawn time. Values are bounded and validated. Clients receive the same rules used by server physics and prediction. Modded sessions do not earn standard achievements or update standard profile records.

A behavior hook looks like this:

```json
{
  "id": "heal_after_kill",
  "event": "kill",
  "target": "actor",
  "actions": [{ "type": "heal", "amount": 15.0 }]
}
```

Events are `tick`, `spawn`, `kill`, `death`, `shot`, `level_up`, `evolve`, `capture`, `round_start`, and `round_end`. Targets are `actor`, `all_players`, and `alive_players`. `every_ticks` controls frequency; `cooldown_ticks` limits each actor independently. `conditions` combine health fraction, level, class, team, bot status, or module counters, with `any` and `not` expressions.

Actions can heal or damage, award XP and score, change class or team, apply an impulse, teleport, update a counter, announce text, finish a round, spawn a polygon target, fire a bullet, or set movement/aim/fire controls. Teleports and spawned entities respect walls and arena limits. Bot control hooks run after bot decisions and before physics. Effects are validated as a batch before application. The Rust definitions in [tankyard-mods](../crates/tankyard-mods/src/lib.rs) specify every field and numeric bound.

JSON behavior runs on the server through bounded hooks. A module has at most 64 hooks, 16 conditions and actions per hook, four nested condition levels, 512 effect commands and 10,000 condition operations per tick, 4,096 counters, and 16,384 cooldown entries. Faulty modules are disabled without applying the failing batch. Hooks do not recursively dispatch their own effects. Reloading or restarting a round resets runtime state.

Trusted Rust extensions implement `tankyard_core::modding::ServerPlugin`, receiving a read-only `World`, a `HookEvent`, and validated `Commands`. Register a factory with `UdpServer::register_plugin`; the factory recreates the extension each round. [The example](../crates/tankyard-server/examples/plugin.rs) heals on kills. Compile it with `cargo check --locked -p tankyard-server --example plugin`. Native extensions execute inside the server process and must be trusted: panic handling does not stop infinite loops or restrict filesystem access. Joining clients receive validated data only and never execute server code.

Tank definitions use the same schema as `assets/content/classes.json`. Stable IDs, tier levels, parents, and upgrade edges must agree. Tiers unlock at 1, 15, 30, 45, and 55. Vanilla classes must have distinct behavior; mods may add alternate profiles. Eight multipliers mean health, regeneration, body damage, bullet speed, penetration, bullet damage, reload, and movement. Bigger reload values shorten cooldowns. Each skill has seven levels; a level-45 tank earns 33 points, and a level-55 tank earns 38. Trap launch distance uses the weapon-speed slot.

Barrel offsets use tank-local coordinates. Angles are radians; lengths/speeds are arena units; timers are seconds. `profile` selects `auto`, `rectangle`, `trapezoid`, `trap`, or `rocket`. `target_arc` limits automatic aiming. Co-located emitters with identical dimensions share one visible cannon. `body_style` selects `auto`, `circle`, `smasher`, `landmine`, or `spike`; `body_scale` changes both model and collision size.

Textures are optional PNG body skins. A texture entry has an `id` and relative `file`; a class references it through `body_texture`. Paths cannot leave the pack directory, including through symlinks. At most 16 images of up to 256 by 256 pixels are allowed. The server decodes PNGs and sends bounded RGBA data. Native MSAA smooths geometry; shape quality controls circle detail. Anisotropic filtering applies to mod textures on supported OpenGL drivers, with linear filtering as the fallback. MSAA changes need a restart.

Maps use normalized walls, spawn regions, objectives, and bases. World dimensions scale the arena. Custom map IDs appear in the map picker. Seeded procedural maps are generated on the server, and snapshots carry their walls.

At most 16 packs may be enabled. Each manifest and the complete serialized data bundle are limited to 1 MiB. The content limits are 1,024 classes and 256 maps. Dependencies, references, finite numbers, and array sizes are checked before activation. A failed server reload leaves the active session intact. Protocol 6 transfers the validated catalog and textures over a reliable channel with decompression limits; behavior files stay on the server.

Export the actual vector models without a display:

```sh
cargo run --locked -p tankyard-client --bin tankyard-models -- --output artifacts/models/tanks.svg
```

Use `--classes triple_twin,battleship,auto_smasher --columns 3` for a smaller sheet. `tankyard-tools balance` reports weapon output; `tournament --bots 24 --ticks 18000` runs an autonomous arena. Damage totals help find outliers; they do not measure match strength.

Model references include the supplied screenshots and the Diep community pages for [Triple Twin](https://diepio-archive.fandom.com/wiki/Triple_Twin), [Battleship](https://diepio.fandom.com/wiki/Battleship), [Hunter](https://diepio.fandom.com/wiki/Hunter), [Skimmer](https://diepio.fandom.com/wiki/Skimmer), [Auto 3](https://diepio.fandom.com/wiki/Auto_3), and [Auto Smasher](https://diepio-archive.fandom.com/wiki/Auto_Smasher). Tank Yard draws its own geometry. Server event hooks follow the general pattern documented by [Garry's Mod](https://wiki.facepunch.com/gmod/Hook_Library_Usage); it does not load Garry's Mod addons.

## Factory minions and drone limits

A barrel using `"projectile": "Minion"` launches a tiny tank with a gun. It’s a real, server-simulated projectile, not a second player and not client-side damage. Minions look for visible targets when left alone; holding fire sends them into a moving ring around the cursor, where they shoot at the pointer. Right-click pulls them back to their owner.

Set `drone_limit` on the class. `Drone` and `Minion` both take a slot from the **same** pool. A Factory-style class usually wants six; a standard controller usually wants eight. The limit is per tank, not a global match setting. A dead minion frees its slot, and the spawning barrel fills it again on its own. Its `cooldown`, `speed`, `damage`, `radius`, and `penetration` still come from the usual barrel fields.

Don't fake extra minions in a renderer plugin. They'll look cute and do absolutely jack shit to the server's hit detection.
