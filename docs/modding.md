# Modding

Tank Yard has two kinds of packs, and both are a folder or a `.zip` archive with a `mod.json` at the root:

- **Lua script packs** (manifest `api_version` 2) can change almost everything about a match: custom modes, scoring, physics nudges, new tanks and maps, timed events and rule changes. Scripts run on LuaJIT inside a sandbox with no file, network, process or native-code access.
- **Data packs** (manifest `api_version` 1) add or replace tanks, maps, rules and textures, and can attach bounded JSON behavior hooks.

In the game, open **Mods** and press **Browse for a pack…** to pick a `.zip` (or the `mod.json` inside a pack folder) in your system's file chooser. You can also paste a `.zip` or folder path and press **Import**, or drop a `.zip` onto the window. Imported packs are copied into your library (`…/Tank Yard/mods` in your application data folder; **Open mods folder** shows it), and a pack that passes review is enabled straight away. Pack folders or archives you put in the library yourself appear after **Refresh**. Folders in a `mods` directory next to the game are listed too; a library pack with the same ID replaces a bundled one. **Remove** deletes an imported pack. **View files** browses everything in a pack, straight from its folder or `.zip` without extracting it: scripts, JSON and TOML with line numbers and syntax colors, Markdown rendered with headings, lists, tables and code (the **Rendered** switch shows it as written), art and models as previews (the **Preview** switch shows an SVG's code), and anything else as a hex dump. Drag over code to highlight it and press Cmd/Ctrl+C to copy; code blocks in rendered Markdown have a copy button. Clicking a finding in **Safety review** opens its file at that line. Enabled packs are remembered by ID, so they stay enabled across restarts, updates and moves, and apply to offline games and servers you host. `--mod` on the command line loads packs for that run only and leaves your saved list alone. Community servers load packs with `--mod path` or the `mods` setting; joining players receive only validated data and never run server scripts.

The Mods page lists every pack with its icon, name, version, creator and safety status. Select one to see its description, license, tags, contents (tanks, maps, scripts, textures, hooks), size and location, with buttons for its source code, download page and issue tracker. Packs describe themselves in an optional `tankyard.toml`:

```toml
[mod]
name = "Bounty Hunt"
version = "1.2.0"
description = "Hunt the leader for bonus score."
license = "MIT"
icon = "icon.svg"            # any supported image, editor file or model
tags = ["mode", "pvp"]

[creator]
name = "Your Name"
url = "https://example.com"

[links]
source = "https://github.com/you/bounty-hunt"   # if open source
download = "https://example.com/bounty-hunt.zip"
issues = "https://github.com/you/bounty-hunt/issues"
homepage = "https://example.com"
```

Every field is optional, and `tankyard.toml` never changes gameplay. Links must be `http://` or `https://`; anything else is rejected so a pack can never open a program or local file.

**View files** shows a pack's contents to anyone who installs it. To keep some or all of them private, add a `[source]` section:

```toml
[source]
viewable = false              # hide every file's contents
private = ["lib/", "*.secret"] # or hide only these
```

Private files are still listed, with a lock, but their contents aren't shown, and the Mods page says the pack has private files. `private` takes exact paths, `folder/` for everything in a folder, `*` for any part of a name and `**` for any number of folders (`**/keys.json`). If `tankyard.toml` can't be read, every file is treated as private. This only affects the file browser: the files are still inside the pack for anyone who unzips it, and the safety review still reads every file before the pack can run.

```sh
cargo run --locked -p tankyard-tools -- validate-mods --mod mods/bounty_hunt --mod mods/vampire
cargo run --locked -p tankyard-tools -- scan-mod mods/bounty_hunt path/to/pack.zip
cargo run --release --locked -p tankyard-tools -- mod-bench --mod mods/gravity_wells --bots 40
./target/release/tankyard --offline --mod mods/gravity_wells
./target/release/tankyard-server --mod mods/bounty_hunt
```

## Review before a pack can run

Every pack is reviewed statically before it can be enabled. The review never runs the pack. It tokenizes each Lua file properly, so comments, ordinary text, table keys such as `{ os = 1 }`, fields such as `player.os` and local variables that share a library's name are not reported. Findings have three levels:

- **Danger** — attempts to reach the computer: `os.execute`, `io.popen`, `io.open`, `os.remove`, `os.getenv`, `loadfile`, `dofile`, `package.loadlib`, the LuaJIT FFI (`require "ffi"`, `ffi.load`, C declarations of `system`, `CreateProcess`, `LoadLibrary`, `dlopen`, `mprotect` and similar), debug-library escapes, network modules, embedded bytecode, executables or native libraries of any platform (PE, ELF, Mach-O, universal binaries, WebAssembly, shell and batch scripts), and code that decodes data and runs it.
- **Review** — unusual content: shell, PowerShell, AppleScript, `launchctl`, `schtasks`, registry, `curl … | sh`, reverse-shell and keychain or SSH-key strings for Windows, macOS and Linux; long base64 or hex payloads; large byte arrays; escape-encoded binary strings; nested archives; files whose contents are not recognized; `load`/`loadstring`.
- **Note** — harmless but useful advice, such as `os.time()` being unavailable (use `tankyard.time()`).

A pack with only notes shows **Reviewed** and can be enabled directly. Anything else opens the findings, with file and line, and requires **Enable anyway**; packs that try to reach the computer also require ticking *I trust this pack's author*. Approval is bound to the pack's exact contents: if the pack changes, it is held back until reviewed again. `tankyard --mod` prints findings and refuses flagged packs unless `--accept-mod-risk` is passed. `tankyard-tools scan-mod` exits with 1 for review findings and 2 for danger findings.

The sandbox blocks all of these attempts regardless of the review; the review tells you a pack was written to try.

## Lua script packs

```json
{
  "api_version": 2,
  "id": "bounty_hunt",
  "name": "Bounty Hunt",
  "version": "1.0",
  "author": "You",
  "description": "The leader carries a bounty.",
  "scripts": ["main.lua"]
}
```

`scripts` lists entry files, run in order. Other `.lua` files in the pack are loaded with `require("lib.util")` (`lib/util.lua` or `lib/util/init.lua`); nothing outside the pack can be required. Script packs define behavior in Lua, so they cannot also carry JSON `hooks`. `dependencies`, `classes`, `maps`, `rules`, `mode_name` and `textures` work exactly as in data packs. See `mods/bounty_hunt`, `mods/gravity_wells` and `mods/sprinkler`.

### Events

```lua
tankyard.on("kill", function(e)
  tankyard.heal(e.actor, 15)
end)
tankyard.every(60 * 30, function(tick) tankyard.announce("Half a minute passed") end)
```

| Event | `actor` | `target` | `value` |
| --- | --- | --- | --- |
| `tick` | 0 | 0 | 0 |
| `spawn` | the tank | 0 | 0 |
| `kill` | the killer | the victim | XP awarded |
| `death` | the victim | the killer (0 for the arena) | 0 |
| `damage` | the damaged tank | the attacker | damage dealt |
| `shot` | the shooter | 0 | projectile kind |
| `level_up` | the tank | 0 | new level |
| `evolve` | the tank | 0 | 0 |
| `capture` | the scorer | 0 | team |
| `round_start`, `round_end` | 0 | 0 | 0 |

Each event table also has `type`, `x`, `y` and `tick`. `tankyard.every(n, f)` runs `f(tick)` every `n` ticks (60 ticks per second). A round starts with a fresh script state; `tankyard.state` is a table you can use for your own data during the round.

### Reading the world (inside events)

| Function | Returns |
| --- | --- |
| `tankyard.tanks()` / `tankyard.players()` | every tank / only humans |
| `tankyard.tank(id)` | one tank or `nil` |
| `tankyard.nearest(x, y, {enemy_of=team, exclude=id, humans=true})` | the nearest living tank and its distance |
| `tankyard.shapes(x, y, radius)` / `tankyard.projectiles(x, y, radius)` | up to 256 nearby shapes / projectiles |
| `tankyard.bosses()`, `tankyard.walls()`, `tankyard.objectives()` | arena contents |
| `tankyard.arena()` | `width`, `height`, `mode`, `teams`, `map`, `round`, `elapsed`, `finished`, `winner`, `scores`, `bounds` |
| `tankyard.base(team)` | a team base rectangle or `nil` |
| `tankyard.class(id)`, `tankyard.classes()` | tank definitions (`name`, `tier`, `level`, `upgrades`, `parents`, `ability`) |
| `tankyard.tick()`, `tankyard.time()`, `tankyard.distance(a, b)` | timing and helpers |

Tank tables carry `id`, `name`, `class`, `team`, `x`, `y`, `vx`, `vy`, `aim`, `radius`, `health`, `max_health`, `level`, `xp`, `score`, `kills`, `deaths`, `alive`, `bot`, `points`, `stats` (1–8), `shield`, `protection`, `speed_scale`, `reload_scale`, `damage_scale` and `in_base`. World access exists only while an event runs; a function saved for later cannot reach the world outside an event.

### Changing the match

Every request is validated, and a pack's requests in one event either all apply or, if any is invalid, none do and the pack is disabled with its error shown.

| Function | Effect |
| --- | --- |
| `heal(id, n)`, `damage(id, n)`, `kill(id)` | health |
| `award(id, xp, score)`, `set_score(id, n)`, `set_team_score(team, n)` | progress and scoring |
| `set_class(id, class)`, `set_level(id, level)`, `raise_stat(id, stat)` | builds (`stat` is 1–8 or a name such as `"reload"`) |
| `set_team(id, team)` | teams |
| `set_scales(id, {speed=, reload=, damage=})` | movement, reload and outgoing damage multipliers |
| `set_armor(id, m)`, `set_shield(id, s)`, `set_protection(id, s)` | incoming damage, shield seconds, invulnerability seconds |
| `push(id, vx, vy)`, `set_velocity(id, vx, vy)`, `teleport(id, x, y)` | movement (world units; teleports avoid walls) |
| `fire(id, {speed, damage, radius, lifetime, angle})` | an extra bullet |
| `control(id, {x=, y=, aim=, fire=, ability=})` | drive a tank this tick (bots, custom modes) |
| `spawn_shape(x, y, {sides, health, xp})`, `spawn_boss(name)` | arena contents |
| `clear_projectiles(id)` | remove a tank's projectiles (`0`: all except base guardians) |
| `announce(text)`, `finish_round(team, reason)` | match flow |

### Defining content from Lua

During loading, `tankyard.define_class(t)`, `tankyard.define_map(t)`, `tankyard.set_rules(t)`, `tankyard.mode_name(s)` and `tankyard.texture(id, "file.png")` declare content with the same fields as `classes.json` and `maps.json`. `tankyard.base_class(id)` and `tankyard.base_map(id)` return editable copies of existing definitions, so a new tank can be attached to the tree by also redefining its parent:

```lua
local parent = tankyard.base_class("spray")
table.insert(parent.upgrades, "sprinkler")
tankyard.define_class(parent)
local tank = tankyard.base_class("fan")
tank.id, tank.name, tank.parents, tank.upgrades = "sprinkler", "Sprinkler", { "spray" }, {}
tankyard.define_class(tank)
```

Definitions are validated like data packs and ignored while a round runs, so the same file serves both phases.

### Menu, theme and wording

Packs can restyle the game itself, like a resource pack. These apply only on the player's machine, to the menus and HUD; they are never sent to servers or other players.

```lua
tankyard.menu({
  title = "OVERCLOCK",                    -- replaces "Tank Yard"
  subtitle = "The rules change every 40 seconds",
  splashes = { "Pass the bomb!", "First to 5,000!" },  -- one is shown per launch
  showcase = "vector",                    -- the tank spinning above the menu
  background = "art/menu_tile.svg",       -- an image in the pack, tiled
  ambient = false,                        -- hide the drifting arena backdrop
})
tankyard.theme({                          -- "#rgb", "#rrggbb" or { r, g, b }
  accent = "#b14cff", background = "#140b24", surface = "#21143a",
  line = "#4a2f7a", text = "#f3ebff", muted = "#b4a3d4", highlight = "#ffd34d",
})
tankyard.text({ ["Play offline"] = "Solo run" })  -- exact interface text
```

Data packs use the same fields as `"menu"`, `"theme"` and `"text"` objects in `mod.json`. Later packs win field by field; splashes add up (64 at most) and text replacements merge (256 at most). Unset theme colors keep the player's light or dark theme.

### Sandbox and limits

Scripts run on LuaJIT's interpreter, one isolated state per pack, recreated every round. They get the base functions, `string`, `table`, `math`, `bit` and `coroutine`. Removed: `os`, `io`, `debug`, `ffi`, `jit`, `package`, `load`, `loadstring`, `loadfile`, `dofile`, `getfenv`, `setfenv`, `collectgarbage`, `newproxy` and `string.dump`; only text source is accepted. `math.random` is seeded per round, and `print` writes to the pack's log (shown in the server console).

The JIT trace compiler is off inside the sandbox because compiled traces skip LuaJIT's instruction hook; the interpreter keeps every instruction accountable. Each event handler may run 500,000 instructions and each pack 1,000,000 per tick (20,000,000 while loading), with 64 MiB of memory. `pcall`, `xpcall` and `coroutine.resume` cannot swallow a budget error. C-level library calls are bounded too: `string.rep` results to 1 MiB, pattern matching to 64 KiB subjects and a quantifier cost limit, and `table.sort` to 100,000 elements. Archives are limited to 8 MiB, 256 files, 2 MiB per file and safe relative names; links, encrypted entries and size-header mismatches are rejected.

## Data packs

API version 1 manifests require `api_version`, `id`, `name`, and `version`. Optional fields are `dependencies`, `classes`, `maps`, `rules`, `mode_name`, `textures`, `hooks`, `menu`, `theme` and `text`. Dependencies load first; missing dependencies and cycles are rejected. Definitions replace an existing ID or add a new one. Among packs whose dependencies are ready, command-line order decides loading order. Later definitions win. A supplied `rules` object replaces the complete rule profile; omitted fields take standard defaults. Packs without a rules object keep the configured server physics. A custom `mode_name` labels the underlying mode, and hooks provide its extra behavior.

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

Events are `tick`, `spawn`, `kill`, `death`, `shot`, `level_up`, `evolve`, `capture`, `round_start`, `round_end`, and `damage`. Targets are `actor`, `all_players`, and `alive_players`. `every_ticks` controls frequency; `cooldown_ticks` limits each actor independently. `conditions` combine health fraction, level, class, team, bot status, or module counters, with `any` and `not` expressions.

Actions can heal or damage, award XP and score, change class or team, apply an impulse, teleport, update a counter, announce text, finish a round, spawn a polygon target, fire a bullet, set movement/aim/fire controls, and use every Lua command above (`set_level`, `raise_stat`, `set_scales`, `set_armor`, `set_shield`, `set_protection`, `set_velocity`, `kill`, `set_score`, `set_team_score`, `spawn_boss`, `clear_projectiles`). Teleports and spawned entities respect walls and arena limits. Bot control hooks run after bot decisions and before physics. Effects are validated as a batch before application. The Rust definitions in [tankyard-mods](../crates/tankyard-mods/src/lib.rs) specify every field and numeric bound.

JSON behavior runs on the server through bounded hooks, alongside Lua packs. A module has at most 64 hooks, 16 conditions and actions per hook, four nested condition levels, 512 effect commands and 10,000 condition operations per tick, 4,096 counters, and 16,384 cooldown entries. Faulty modules are disabled without applying the failing batch. Hooks do not recursively dispatch their own effects. Reloading or restarting a round resets runtime state.

Trusted Rust extensions implement `tankyard_core::modding::ServerPlugin`, receiving a read-only `World`, a `HookEvent`, and validated `Commands`. Register a factory with `UdpServer::register_plugin`; the factory recreates the extension each round. [The example](../crates/tankyard-server/examples/plugin.rs) heals on kills. Compile it with `cargo check --locked -p tankyard-server --example plugin`. Native extensions execute inside the server process and must be trusted: panic handling does not stop infinite loops or restrict filesystem access. Joining clients receive validated data only and never execute server code.

Tank definitions use the same schema as `assets/content/classes.json`. Stable IDs, tier levels, parents, and upgrade edges must agree. Tiers unlock at 1, 15, 30, 45, and 55. Vanilla classes must have distinct behavior; mods may add alternate profiles. Eight multipliers mean health, regeneration, body damage, bullet speed, penetration, bullet damage, reload, and movement. Bigger reload values shorten cooldowns. Each skill has seven levels; a level-45 tank earns 33 points, and a level-55 tank earns 38. Trap launch distance uses the weapon-speed slot.

Barrel offsets use tank-local coordinates. Angles are radians; lengths/speeds are arena units; timers are seconds. `profile` selects `auto`, `rectangle`, `trapezoid`, `trap`, or `rocket`. `target_arc` limits automatic aiming. Co-located emitters with identical dimensions share one visible cannon. `body_style` selects `auto`, `circle`, `smasher`, `landmine`, or `spike`; `body_scale` changes both model and collision size.

Textures are optional body skins. A texture entry has an `id` and relative `file`; a class references it through `body_texture`. Paths cannot leave the pack directory, including through symlinks. At most 16 textures are allowed; sources up to 4096 pixels on a side are shrunk to 256 by 256 keeping their shape. The server decodes art and sends bounded RGBA data, so players need nothing installed. A texture's right side faces where the tank aims.

| Kind | Formats |
| --- | --- |
| Images | PNG, JPEG, GIF (first frame), BMP, TGA, WebP, ICO, ICNS |
| Vector art | SVG (rendered crisp at texture size; scripts and external files are ignored) |
| Icon and paint editors | Aseprite (`.ase`, `.aseprite`, first frame), Photoshop (`.psd`, flattened), Krita (`.kra`, merged image) |
| 3D models | glTF/GLB, OBJ (with `.mtl` colors), STL, PLY (with vertex colors) |

Models from Blender, Maya, 3ds Max, SketchUp, MagicaVoxel, Tinkercad and other modeling tools load through those exports. They render top-down into a sprite with flat lighting and the game's dark outline, so they match the built-in tanks. glTF and OBJ treat +Y as up and +Z as forward; STL treats +Z as up and +X as forward. Prefer `.glb` or embedded glTF buffers: a separate `.bin` buffer works, but the safety review flags every `.bin` file for a manual look. FBX and `.blend` are proprietary or version-specific; export glTF instead. Native MSAA smooths geometry; shape quality controls circle detail. Anisotropic filtering applies to mod textures on supported OpenGL drivers, with linear filtering as the fallback. MSAA changes need a restart.

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
