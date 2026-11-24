# Modding

Tank Yard mods are packs: a folder or `.zip` with a `mod.json` at its root. A pack can add game modes, scoring rules, tanks, maps, bosses on a timer, HUD widgets, skins and a restyled menu. All of it runs on the server, in a sandbox, and players who join a modded server need nothing installed.

- [Quick start](#quick-start)
- [Kinds of packs](#kinds-of-packs)
- [Installing and managing packs](#installing-and-managing-packs)
- [Safety review](#safety-review)
- [Writing scripts: Luau or legacy LuaJIT](#writing-scripts-luau-or-legacy-luajit)
- [How a script runs](#how-a-script-runs)
- [Script API reference](#script-api-reference)
- [Sandbox and limits](#sandbox-and-limits)
- [Workshop plugins](#workshop-plugins)
- [Data packs and JSON hooks](#data-packs-and-json-hooks)
- [Content reference: tanks, maps, textures](#content-reference-tanks-maps-textures)
- [Recipes](#recipes)
- [Tools for pack authors](#tools-for-pack-authors)
- [Trusted Rust server plugins](#trusted-rust-server-plugins)
- [Troubleshooting](#troubleshooting)

## Quick start

Make a folder with two files.

`mod.json`:

```json
{
  "api_version": 2,
  "id": "first_blood",
  "name": "First Blood",
  "version": "1.0.0",
  "author": "You",
  "description": "The first kill of every round earns a bonus.",
  "scripts": ["main.lua"]
}
```

`main.lua`:

```lua
--!strict
local claimed = false

tankyard.on("round_start", function()
  claimed = false
end)

tankyard.on("kill", function(event)
  if claimed or event.actor == 0 then
    return
  end
  claimed = true
  local hunter = tankyard.tank(event.actor)
  tankyard.award(event.actor, 2000, 500)
  tankyard.announce(`First blood: {if hunter then hunter.name else "someone"}!`)
end)
```

Try it:

```sh
tankyard --offline --mod path/to/first_blood
```

Or open **Mods** in the game, press **Browse for a pack…** and pick the folder's `mod.json`. When it works, zip the folder (with `mod.json` at the top of the archive) to share it.

## Kinds of packs

| Kind | `api_version` | What it can do |
| --- | --- | --- |
| **Script pack** (recommended) | `2` | Everything: Luau scripts react to events, read the world, change the match, define tanks and maps, and show HUD widgets. It can also carry the data that data packs carry. |
| **Workshop plugin** | `2` with `editor` | Tools and checks for the [Arena Workshop](map-editor.md#workshop-plugins). It runs only in the editor, and a pack with nothing else never loads into a match. See [Workshop plugins](#workshop-plugins). |
| **Data pack** | `1` | Tanks, maps, rules, textures, menu styling and bounded JSON behavior hooks, with no code. Good for balance mods, skins and simple rules. |
| **Staff credential pack** | `1` with a `staff` section | Unlocks moderation tools in the game client for Tank Yard staff. It carries no game content and never loads into a match. See [Creator administration](creator-administration.md#staff-credential-packs). |

### `mod.json`

| Field | Packs | Meaning |
| --- | --- | --- |
| `api_version` | all | `2` for script packs, `1` for data packs. Required. |
| `id` | all | Lowercase letters, digits and `_`. Required. Enabled packs are remembered by ID, and a pack in your library replaces a bundled pack with the same ID. |
| `name`, `version` | all | Shown in the Mods page and server lists. Required (64 and 32 characters at most). |
| `author`, `description` | all | Optional. |
| `scripts` | 2 | Entry files, run in order (16 at most). |
| `editor` | 2 | [Workshop plugin](#workshop-plugins) files, run in order in the map editor (16 at most). |
| `dependencies` | all | IDs of packs that must load first. A missing dependency or a cycle stops the pack. |
| `classes`, `maps` | all | Tank and map definitions ([reference](#content-reference-tanks-maps-textures)). |
| `rules` | all | A complete [rule profile](#rules). |
| `mode_name` | all | A label for the mode, up to 48 characters, such as `"Bounty Hunt"`. |
| `textures` | all | `[{ "id": "skin", "file": "art/skin.png" }]`, 16 at most. |
| `hooks` | 1 | [JSON behavior hooks](#hooks). Script packs write behavior in Luau instead. |
| `menu`, `theme`, `text` | all | [Menu, theme and wording](#menu-theme-and-wording) on the player's machine. |

### `tankyard.toml`

An optional file that describes the pack for the Mods page. It never changes gameplay.

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

[source]
viewable = true                # false hides every file's contents
private = ["lib/", "*.secret"] # or hide only these
```

Every field is optional. Links must be `http://` or `https://`, so a pack can never open a program or a local file.

**View files** in the game shows a pack's contents to anyone who installs it. `[source]` hides contents there: `private` takes exact paths, `folder/` for a whole folder, `*` for any part of a name and `**` for any number of folders (`**/keys.json`). Private files are still listed, with a lock. If `tankyard.toml` can't be read, every file is treated as private. Hiding a file only affects the viewer: the file is still in the pack for anyone who unzips it, and the safety review still reads it.

Folder packs contain ordinary disk files and folders. Pipes, devices and other special files are rejected, and directory entries count toward the pack limits even when they are ignored metadata.

### Layout

```
my_pack/
├── mod.json
├── tankyard.toml        optional
├── icon.svg             optional, named in tankyard.toml
├── main.lua             entry script
├── lib/
│   └── util.lua         require("lib.util")
└── art/
    └── skin.png         textures and menu backgrounds
```

## Installing and managing packs

In the game, open **Mods**:

- **Browse for a pack…** picks a `.zip`, or the `mod.json` inside a folder, with your system's file chooser. You can also paste a path and press **Import**, or drop a `.zip` onto the window.
- Imported packs are copied into your library (`…/Tank Yard/mods` in your application data folder; **Open mods folder** shows it). Packs you copy there yourself appear after **Refresh**, and folders in a `mods` directory next to the game are listed too.
- A pack that passes review is enabled straight away. At most 16 packs can be enabled. Enabled packs apply to offline games and to servers you host, and stay enabled across restarts and updates.
- Select a pack to see its description, license, tags, contents (tanks, maps, scripts, textures, hooks), size and location, with buttons for its source, download page and issue tracker. **Remove** deletes an imported pack.
- **View files** browses a pack without extracting it: code with line numbers and syntax colors, Markdown rendered, art and models as previews, and anything else as a hex dump. Select code and press Cmd/Ctrl+C to copy it. Clicking a finding in **Safety review** opens its file at that line.

From the command line, `--mod path` loads packs for one run without changing your saved list:

```sh
tankyard --offline --mod mods/gravity_wells --mod mods/bounty_hunt
tankyard-server --mod mods/bounty_hunt          # or the server's `mods` setting
```

Players who join a modded server receive only validated data (tanks, maps, rules, textures and HUD widgets). Scripts and hooks stay on the server, and joining players never run them. Modded sessions don't earn standard achievements or change standard profile records.

## Safety review

Every pack is reviewed before it can be enabled. The review reads every file and never runs anything. It tokenizes Lua properly, so comments, strings, table keys such as `{ os = 1 }`, fields such as `player.os` and local variables that share a library's name are not reported.

- **Danger:** attempts to reach the computer. This covers `os.execute`, `io.popen`, `io.open`, `os.remove`, `os.getenv`, `loadfile`, `dofile`, `package.loadlib`, LuaJIT's FFI (`require "ffi"`, `ffi.load`, C declarations of `system`, `CreateProcess`, `LoadLibrary`, `dlopen`, `mprotect` and similar), debug-library escapes, network modules, embedded bytecode, executables or native libraries for any platform, shell and batch scripts, and code that decodes data and runs it.
- **Review:** unusual content. This covers shell, PowerShell, AppleScript, `launchctl`, `schtasks`, registry, `curl … | sh`, reverse-shell, keychain and SSH-key strings, long base64 or hex payloads, large byte arrays, escape-encoded binary, nested archives, unrecognized files, and `load`/`loadstring`.
- **Note:** harmless advice, such as `os.time()` being unavailable (use `tankyard.time()`).

A pack with only notes shows **Reviewed** and can be enabled directly. Anything else shows the findings with file and line, and needs **Enable anyway**. Packs that try to reach the computer also need *I trust this pack's author*. Approval is tied to the pack's exact contents, so a changed pack is held back until it's reviewed again. `tankyard --mod` prints findings and refuses flagged packs unless you pass `--accept-mod-risk`.

The sandbox blocks all of these attempts whatever the review says. The review tells you that a pack was written to try.

## Writing scripts: Luau or legacy LuaJIT

Scripts run on [Luau](https://luau.org), compiled to machine code where the platform allows. Tank Yard used a LuaJIT sandbox before 0.12. Both styles of code run on the same runtime, so pick by what you're writing:

| | Modern Luau (recommended) | Legacy LuaJIT style |
| --- | --- | --- |
| For | New packs | Packs written before 0.12, or shared Lua 5.1 code |
| Syntax | Luau: `continue`, `+=`, `if … then … else` expressions, string interpolation, type annotations | Lua 5.1 plus LuaJIT's extras: Lua 5.3 bitwise operators, `goto continue` |
| Libraries | Luau's (`table.find`, `string.split`, `math.clamp`, `buffer`, …) and `bit` | `bit`, `bit32` and the Lua 5.1 libraries |
| Tooling | [Luau's type checker and language server](https://luau.org/getting-started) work on pack files | Any Lua 5.1 editor |

You can mix both in one pack, even in one file: there's no switch to set. Write new code in modern Luau and leave old code alone until you want to change it.

### Modern Luau

```lua
--!strict
-- Types are optional and checked only by your editor; the game ignores them.
type Tank = { id: number, name: string, team: number, score: number, alive: boolean }

local STREAK_BONUS = 250
local streaks: { [number]: number } = {}

tankyard.on("kill", function(event)
  if event.actor == 0 then return end
  streaks[event.actor] = (streaks[event.actor] or 0) + 1
  local streak = streaks[event.actor]
  if streak % 5 ~= 0 then return end
  tankyard.award(event.actor, 0, STREAK_BONUS * streak // 5)
  local tank = tankyard.tank(event.actor) :: Tank?
  tankyard.announce(`{if tank then tank.name else "Someone"} is on a {streak}-kill streak`)
end)

tankyard.on("death", function(event)
  streaks[event.actor] = nil
end)

tankyard.every(60, function()
  for _, tank in tankyard.players() do       -- iterate a table directly
    if not tank.alive then continue end
    local streak = streaks[tank.id] or 0
    if streak > 0 then
      tankyard.hud("streak", { text = `Streak {streak}`, x = 0.98, y = 0.06, align = "right" }, tank.id)
    else
      tankyard.hud("streak", nil, tank.id)
    end
  end
end)
```

What Luau adds:

- **Syntax:** `continue`, compound assignment (`x += 1`, `s ..= "!"`), `if … then … else …` expressions, string interpolation (`` `{name} has {score}` ``), floor division `//`, type annotations and casts (`::`), and iteration over a table directly (`for k, v in t do`).
- **Libraries:** `bit32`, `utf8`, `buffer`, `table.find`, `table.clear`, `table.create`, `table.clone`, `table.freeze`, `table.pack`/`table.unpack`, `string.split`, `string.pack`/`string.unpack`, `math.clamp`, `math.round`, `math.sign`, `math.noise`, `coroutine.close` and `typeof`.
- **Speed:** scripts compile to machine code (except on platforms that forbid it), library calls such as `math.floor` run without a function call, and a pack is compiled once per server instead of every round. A pack that replaces a library function (`math.floor = …`) or edits `_G` still sees its change; Luau only skips the shortcut for those names.

For editor support, give the language server a definition file for the `tankyard` global, or start files with `--!nonstrict` until you have one.

### Legacy LuaJIT packs

Packs written for the LuaJIT sandbox run unchanged and give the same results:

- Numbers print the same way (`tostring(0.1)` is `0.1`; `%.14g` throughout).
- `string.format`, `%q`, `string.rep` with a separator and `gsub` replacement escapes behave as before.
- `table.sort` orders equal elements exactly as before.
- `math.max`/`math.min` treat NaN and `-0` the same.
- `#` ignores `__len` on tables, `tostring` returns whatever `__tostring` returns, and hexadecimal floats such as `0x1p4` parse.
- `_VERSION` is still `Lua 5.1`.
- The Lua 5.3 bitwise operators (`&`, `|`, `~`, `<<`, `>>`, unary `~`) and the `bit` library keep LuaJIT's signed 32-bit results.
- Library tables stay writable, so helpers such as `function string.trim(s) … end` work in every file.

Some things differ:

- **`goto`:** Luau has no `goto`. The usual LuaJIT pattern, `goto continue` with `::continue::` as the last statement of a loop body, becomes `continue` automatically. Any other `goto` is reported with its line.
- **Error messages** are worded by Luau (`attempt to index nil with 'x'`), with the same file and line prefix.
- **Unspecified behavior** can differ: the order `pairs` visits keys, and `#` on a table with holes. Neither was guaranteed before. Use arrays, or sort the keys, when order matters.
- **Gone from the sandbox:** `ffi`, `jit`, `load`, `loadstring`, `setfenv`/`getfenv`, `collectgarbage` and `string.dump`. None of these worked in the LuaJIT sandbox either.

### Moving a pack to modern Luau

1. Replace `goto continue` … `::continue::` with `continue`.
2. Replace `x = x + 1` with `x += 1`, and `a .. " " .. b` with `` `{a} {b}` `` where it reads better.
3. Replace hand-written helpers with library functions: `table.find`, `table.clone`, `string.split`, `math.clamp`, `math.round`.
4. Add `--!strict` and types as you go, so the type checker can catch mistakes before the game does.
5. Keep using `bit` or the bitwise operators if you need LuaJIT's signed results. `bit32` works on unsigned 32-bit numbers instead.

## How a script runs

A script pack runs in two phases.

**Loading.** When the server or offline game starts, or a mod reload happens, each entry script runs once, top to bottom. This is when definitions apply: `define_class`, `define_map`, `texture`, `set_rules`, `mode_name`, `menu`, `theme` and `text`. Calls to these during a round are ignored, so one file serves both phases. World queries and commands aren't available while loading.

**Rounds.** Each round starts with a fresh copy of every script: the entry scripts run again (definitions are ignored this time), handlers are registered, and events start. Everything a script keeps, including locals and `tankyard.state`, lasts for one round. Use `round_start` for setup.

- `require("lib.util")` loads `lib/util.lua` or `lib/util/init.lua` from the same pack, once per round. Nothing outside the pack can be required.
- World queries and commands work **only inside event handlers and timers**. A function saved for later can't reach the world outside an event.
- Commands queue up while a handler runs. When it returns, they're validated together: either they all apply or, if any is invalid, none do. In that case the pack is disabled and its error is shown on the Mods page and in the server console.
- A Lua error disables the pack the same way, with the file and line.
- `print(...)` writes to the pack's log (the server console), up to 64 lines per round.
- Everything runs at 60 ticks per second on the server. Players see the effects through normal snapshots, so scripts never need to think about networking.

## Script API reference

Everything is in the `tankyard` global. The API version is `tankyard.api` (`2`).

### Events

```lua
tankyard.on("kill", function(event) … end)      -- up to 32 handlers per event
tankyard.every(60 * 30, function(tick) … end)   -- every 30 seconds; up to 64 timers
```

| Event | `actor` | `target` | `value` |
| --- | --- | --- | --- |
| `tick` | 0 | 0 | 0 |
| `spawn` | the tank | 0 | 0 |
| `kill` | the killer (0 for the arena) | the victim | XP awarded |
| `death` | the victim | the killer (0 for the arena) | 0 |
| `damage` | the damaged tank | the attacker (0 if none) | damage dealt |
| `shot` | the shooter | 0 | projectile kind |
| `level_up` | the tank | 0 | new level |
| `evolve` | the tank | 0 | 0 |
| `capture` | the scorer | 0 | team |
| `round_start`, `round_end` | 0 | 0 | 0 |
| `map_event` | the first noticed tank in the trigger's area (0 if none) | 0 | the trigger's number, from 1 |
| `choice` (1.4) | the player who picked | 0 | the choice picked, from 1 |

A `choice` comes from one of the pack's own [HUD widgets](#hud-widgets) with `choices`; its table also has `name`, the widget's ID. Other packs never hear it.

A `map_event` comes from a map trigger whose action is **Script event**. Its table also has `name` (the event name set on the trigger) and `tanks` (the IDs of the noticed tanks inside the trigger's area). See [Map logic](#map-logic).

Every event table also has `type` (the event name), `x`, `y` (where it happened) and `tick`. `tankyard.every(n, f)` calls `f(tick)` every `n` ticks, from 1 to 216,000 (an hour). Kills count only tanks; boss kills are announced but don't fire `kill`. Shots are delivered after every other event in a tick, so a flood of shots can't delay a kill.

### Reading the world

| Function | Returns |
| --- | --- |
| `tankyard.tanks()` | every tank, as [tank tables](#tank-tables) |
| `tankyard.players()` | only human players |
| `tankyard.tank(id)` | one tank, or `nil` |
| `tankyard.nearest(x, y, options)` | the nearest living tank and its distance, or `nil`. `options`: `enemy_of = team` skips that team (team 0 counts as everyone's enemy), `team = n` only that team, `exclude = id`, `humans = true` |
| `tankyard.shapes(x, y, radius)` | up to 256 shapes within `radius` (at most 5,000): `id`, `x`, `y`, `radius`, `health`, `max_health`, `sides`, `xp` |
| `tankyard.projectiles(x, y, radius)` | up to 256 projectiles: `id`, `owner`, `team`, `kind`, `x`, `y`, `vx`, `vy`, `radius`, `damage`, `lifetime`, `guardian` (fired by a base guardian) |
| `tankyard.bosses()` | `id`, `kind` (display name), `x`, `y`, `radius`, `health`, `max_health`, `team` |
| `tankyard.walls()` | rectangles `{ x, y, w, h }` in world units |
| `tankyard.objectives()` | `id`, `x`, `y`, `radius`, `team`, `progress`, `contested`, `carrier` (a tank ID or 0) |
| `tankyard.arena()` | `width`, `height`, `mode` (display name), `mode_key` (such as `"TwoTeams"`), `teams`, `map`, `round`, `elapsed` (seconds), `finished`, `winner`, `scores` (per team), `bounds` (`{ x, y, w, h }`) |
| `tankyard.base(team)` | a team's base rectangle, or `nil` |
| `tankyard.classes()` | every tank class ID |
| `tankyard.class(id)` | `id`, `name`, `tier`, `level`, `ability`, `description`, `drones`, `upgrades`, `parents`; or `nil` |
| `tankyard.map_counter(n)` | map counter `n` (1–8) |
| `tankyard.map_group(group)` | whether map group 1–32 is shown |
| `tankyard.map_pose(group)` | (1.1) where motions have put map group 1–32: `x`, `y` (offset from its place, as fractions of the arena's width and height), `angle` (degrees), `opacity`, `scale` and `tint_strength` (percent), and `tint` (`{ r, g, b }`, 0–255) |
| `tankyard.map_body(group)` | (1.3) where tanks have pushed body group 1–32: `x`, `y` (offset from its place, in world units) and `vx`, `vy` (world units a second); `nil` when the group isn't a [body](#physics-and-lighting) |
| `tankyard.tick()` | the current tick |
| `tankyard.time()` | seconds since the round started (use it instead of `os.time`) |
| `tankyard.distance(a, b)` | the distance between two tables with `x` and `y` |

Positions are world units, with (0, 0) at the arena's top-left corner and y pointing down. Angles are radians, with 0 pointing right.

#### Tank tables

| Field | Meaning |
| --- | --- |
| `id`, `name`, `class`, `team` | identity; team 0 means no team |
| `x`, `y`, `vx`, `vy`, `aim`, `radius` | position, velocity, aim angle, size |
| `pocket` | seconds remaining in the pocket dimension, from 0 to 10; positive means the tank is absent from arena combat and objectives |
| `health`, `max_health`, `shield`, `protection` | health, and the seconds left of shield and spawn protection |
| `level`, `xp`, `score`, `kills`, `deaths`, `points` | progress; `points` are unspent skill points |
| `stats` | the eight skill levels, in the order of `tankyard.stats` |
| `speed_scale`, `reload_scale`, `damage_scale` | multipliers set by packs (1 is normal) |
| `alive`, `bot`, `in_base` | flags |

`tankyard.stats` is `{ "health", "regen", "body", "speed", "penetration", "damage", "reload", "movement" }`.

### Changing the match

All commands need a tank ID where they act on a tank. Values outside the bounds below make the whole batch invalid (see [How a script runs](#how-a-script-runs)).

| Function | Effect and bounds |
| --- | --- |
| `heal(id, amount)`, `damage(id, amount)` | 0 to 10,000 health |
| `kill(id)` | destroys the tank |
| `award(id, xp, score)` | up to 1,000,000 of each; either may be omitted |
| `set_score(id, score)` | up to 1,000,000,000 |
| `set_team_score(team, score)` | team 1–4, up to 1,000,000 |
| `set_class(id, class)` | any loaded class ID |
| `set_level(id, level)` | 1 to 55; skill points follow the normal budget |
| `raise_stat(id, stat)` | `stat` is 1–8 or a name from `tankyard.stats`; free, up to the usual maximum |
| `set_team(id, team)` | 0 to 4 |
| `set_scales(id, { speed =, reload =, damage = })` | movement and reload 0.25–6, outgoing damage 0.25–8 |
| `set_armor(id, multiplier)` | incoming damage multiplier, 0 to 10 (1 restores normal) |
| `set_shield(id, seconds)`, `set_protection(id, seconds)` | 0 to 30 seconds of shield or invulnerability |
| `push(id, vx, vy)`, `set_velocity(id, vx, vy)` | add or set velocity, each axis −2,000 to 2,000 units/s |
| `teleport(id, x, y)` | inside the arena; teleports avoid walls |
| `fire(id, { speed, damage, radius, lifetime, angle })` | an extra bullet: speed 1–1,600 (520), damage 0–10,000 (20), radius 1–60 (8), lifetime 0.05–30 s (2), angle relative to aim (0) |
| `control(id, { x =, y =, aim =, fire =, ability = })` | drives a tank for this tick: movement `x`, `y` from −1 to 1, `aim` in radians (default: current aim), `fire` and `ability` booleans. Runs after bots decide, before physics |
| `spawn_shape(x, y, { sides, health, xp })` | a shape: 3–12 sides (4), health 1–10,000 (60), up to 1,000,000 XP (25) |
| `spawn_boss(name)` | `"Sentinel"`, `"Prism"`, `"Foundry"`, `"Tempest"`, `"Bastion"`, `"Eclipse"`, `"Guardian"`, `"Summoner"`, `"Defender"`, `"Fallen Overlord"`, `"Fallen Booster"`, `"Siege Engine"`, `"Skirmisher"` or `"Trapper"` (case and spaces don't matter) |
| `clear_projectiles(id)` | removes a tank's projectiles; with no ID, every projectile except base guardians' |
| `announce(text)` | a message to everyone, up to 160 characters |
| `finish_round(team, reason)` | ends the round; team 0–4 (0 or `nil` for no winner) |
| `hud(widget_id, widget, id)` | see [HUD widgets](#hud-widgets) |
| `map_signal(channel)` | fires the map's On signal triggers on channel 1–255 (1–32 before 1.7) |
| `set_map_counter(n, value)`, `add_map_counter(n, value)` | sets or adds to map counter 1–255 (1–8 before 1.7); whole numbers, results within ±1,073,741,823 (±99,999 before 1.7) |
| `show_map_group(group, shown)` | shows (`true`, the default) or hides map group 1–32 |
| `reset_map_triggers(group)` | (0.16) makes map group 1–32's triggers start afresh, or every trigger with 0 or `nil`: once-per-round triggers can act again, rest times end, waiting delays are dropped, timers restart from now and sequences go back to their first channel |
| `move_map_group(group, x, y, seconds, curve, mode)` | (1.0) glides map group 1–32 to `x`, `y` from where the map places it, as fractions of the arena's width and height (±2; `0, 0` brings it home), over `seconds` (0–600; 0 or `nil` moves it at once). A glide faster than one arena a second is slowed to that speed. `curve` and `mode` are [easing](#motions) names; leave them out for `"sine"`, `"in_out"` |
| `turn_map_group(group, degrees, seconds, curve, mode)` | (1.0) turns map group 1–32's decor `degrees` further (±3,600) |
| `fade_map_group(group, percent, seconds, curve, mode)` | (1.0) fades map group 1–32's decor to `percent` opacity (0–100) |
| `scale_map_group(group, percent, seconds, curve, mode)` | (1.1) draws map group 1–32's decor at `percent` of its placed size (0–400) |
| `tint_map_group(group, color, percent, seconds, curve, mode)` | (1.1) mixes `color` (`"#rrggbb"` or `{ r, g, b }`) into map group 1–32's decor at `percent` (0–100, default 100); 0 clears the tint |
| `animate_map_group(group, style, size, speed, seconds, curve, mode)` | (1.1) plays `"bob"`, `"sway"`, `"throb"`, `"shake"` or `"orbit"` on map group 1–32's decor. `size` is units for bob, shake and orbit (0–400), degrees for sway (0–180) and percent for throb (0–100); 0 settles it. `speed` is cycles a second (0–12; 0 or `nil` keeps the style's own pace). `seconds` eases the size in or out |
| `stop_map_group(group)` | (1.1) halts map group 1–32's motions and animations where they are and cancels its trip back |
| `shake_view(id, percent, seconds)` | (1.1) shakes one player's view, or every player's with `nil` or 0: `percent` 1–100 (50), `seconds` 0.1–10 (0.5). Players' Camera shake setting and Reduced motion still apply |
| `play_sound(sound, x, y, percent)` | (1.1) plays `"chime"`, `"bell"`, `"alarm"`, `"switch"`, `"whoosh"`, `"rumble"`, `"fanfare"` or `"thump"`, or (2.0.5) any of the newer sounds by its file name, such as `"coin"` or `"power_up"`, at world position `x`, `y`, at `percent` volume (1–100, default 100). Players hear it by distance, at their Effects volume |
| `play_map_path(group, path)` | (1.3) sends map group 1–32 along the map's path 1–32. A path the map lacks, or one with no points, does nothing |
| `set_map_lighting(percent, color, seconds, curve, mode)` | (1.3) darkens the arena to `percent` (0–95) tinted `color` (`"#rrggbb"` or `{ r, g, b }`; default a deep blue), over `seconds` (0–600). 0 brings full light back |
| `shove_map_body(group, x, y)` | (1.3) pushes body group 1–32 at `x`, `y` world units a second (each ±2,000), as a Shove body trigger does. Heavier bodies move less; a group that isn't a body is left alone |
| `fling_tank(id, x, y)` | (1.3) throws a tank at `x`, `y` world units a second (each ±3,000), as a launch pad does; it coasts for a moment before steering takes over |

Commands are capped at 512 per pack per tick.

Functions marked (1.1) need Tank Yard 1.1.0 or later, and those marked (1.3) need 1.3.0. A pack that should also run on older servers can check first: `if tankyard.tint_map_group then ... end`.

#### Map logic

Scripts and map triggers can drive each other. A trigger with a **Script event** action raises `map_event`, and the script answers with the map commands above, which take effect at the start of the next tick, before the map's own timers and areas. Counters and groups changed this way behave exactly as if a trigger had changed them: On counter and On change triggers fire, On shown and On hidden watchers hear group changes, gates open, and hidden blocks stop colliding.

```lua
-- A door that opens for the team that holds the most points.
tankyard.on("map_event", function(event)
  if event.name ~= "vault" then return end
  for _, id in ipairs(event.tanks) do
    local tank = tankyard.tank(id)
    if tank and tank.score >= 5000 then
      tankyard.show_map_group(4, false)  -- group 4 is the vault door
      tankyard.add_map_counter(1, 1)     -- a label shows {1} openings
      return
    end
  end
  tankyard.announce("The vault needs 5,000 points.")
end)
```

A map pack made in the [Arena Workshop](map-editor.md) has no scripts of its own; put the script in a separate pack and enable both, or add a `scripts` list to the exported `mod.json`. [JSON hooks](#hooks) can use the same commands as `map_signal`, `map_counter` and `map_group` actions and listen for `map_event`, but they hear every script event alike; tell names apart in a script.

### HUD widgets

Packs can put text and progress bars on players' screens:

```lua
tankyard.on("tick", function(event)
  -- Everyone sees the round clock; it changes once a second.
  local left = math.max(0, 300 - event.tick // 60)
  tankyard.hud("clock", { text = string.format("%d:%02d", left // 60, left % 60), y = 0.08, size = 22 })
  for _, tank in tankyard.players() do
    -- Each player sees their own meter.
    tankyard.hud("charge", {
      text = "Overdrive", progress = tank.health / tank.max_health,
      x = 0.02, y = 0.9, align = "left", bar_color = "#ffb000", width = 0.18,
    }, tank.id)
  end
end)
```

`tankyard.hud(widget_id, widget, id)` shows `widget` to one player (`id`) or to everyone (no `id`). Calling it again with the same ID replaces the widget, and a `nil` widget removes it.

| Field | Default | Meaning |
| --- | --- | --- |
| `text` | none | up to 120 bytes; a widget needs `text`, a `title`, `progress`, `choices` or a `board` |
| `x`, `y` | `0.5`, `0.2` | position as a fraction of the screen (`y` is the middle of a plain widget's text, or of a panel) |
| `align` | `"center"` | `"left"`, `"center"` or `"right"` of `x` |
| `size` | `18` | text size, 10 to 48, scaled with the interface |
| `color`, `bar_color` | theme colors | `"#rrggbb"` or `{ r, g, b }` from 0 to 255 |
| `progress` | none | a bar below the text, filled from 0 to 1 |
| `width` | `0.2` | the bar's width as a fraction of the screen, 0.02 to 1 |
| (1.4) `style` | `"plain"` | `"plain"` (text with a soft shadow), `"panel"`, `"card"` (a panel with a stripe), `"banner"` (across the whole screen) or `"pill"` |
| (1.4) `title` | none | a smaller line above the text, up to 48 bytes |
| (1.4) `icon` | `"none"` | `"star"`, `"heart"`, `"coin"`, `"clock"`, `"flag"`, `"bolt"`, `"shield"`, `"key"`, `"gem"`, `"trophy"`, `"target"`, `"info"`, `"alert"`, and (1.5) `"crown"`, `"skull"`, `"lock"`, `"check"`, `"cross"`, `"fire"` or `"bomb"` |
| (1.4) `panel`, `accent` | theme colors | the panel's color, and the border, stripe, icon, bar and choice color |
| (1.4) `opacity` | `0.85` | how solid the panel is, 0 to 1 |
| (1.4) `enter` | `"fade"` | how it appears: `"none"`, `"fade"`, `"pop"`, `"slide"` or `"drop"` |
| (1.4) `pulse` | `false` | gently brightens the border to draw the eye |
| (1.4) `segments` | `0` | splits the bar into 2 to 20 pieces, such as lives |
| (1.4) `choices` | none | up to 6 (4 before 1.5) buttons of 1 to 32 bytes; a pick raises the pack's `choice` event |
| (1.5) `layout` | `"auto"` | how choices sit: `"auto"` (in a row, one under another when the row doesn't fit), `"column"` (one under another, all one width) or `"grid"` (two to a row, all one size) |
| (1.5) `glow` | `false` | a soft glow in the accent color around the widget; plain text gets a halo instead of its shadow |
| (1.5) `pin` | none | floats the widget over the arena instead of placing it at `x` and `y`: a tank ID floats it just above that tank and follows it, and `{ x, y }` (or `{ x = ..., y = ... }`) centers it over that spot in world units. It moves with the view and draws under the HUD. A widget over a tank hides while the tank is down or hidden from the viewer |
| (1.5) `point` | `false` | for a pinned widget: while what it floats over is out of view, it waits near the edge of the screen, clear of the HUD, with an arrow toward it, instead of leaving the screen |
| (1.6) `shape` | `"rounded"` | the outline of a panel or card: `"rounded"`, `"square"`, `"chamfer"` (cut corners), `"slant"` (leaning sides), `"tag"` (a point on the right) or `"ribbon"` (notched ends). Pills stay round and banners straight |
| (1.6) `border` | `"line"` | `"line"`, `"none"`, `"thick"`, `"double"`, `"dashed"` or `"corners"` (brackets at the corners) |
| (1.6) `fill` | `"solid"` | `"solid"`, `"gradient"` (the panel color shading into `fill_color` toward the bottom), `"glass"` (see-through with a sheen), or a pattern in `fill_color` over the panel: `"stripes"`, `"grid"` or `"dots"` |
| (1.6) `fill_color` | a shade of the panel | the second color of a gradient or pattern |
| (1.6) `lettering` | `"auto"` | how the words stand out: `"auto"` (a shadow for plain text, a halo with `glow`), `"clean"`, `"shadow"`, `"block"` (a painted block shadow in the accent), `"outline"` or `"glow"` |
| (1.6) `caps` | `false` | shows the title, text and choices in capital letters |
| (1.6) `title_color` | a blend of accent and text | the title's color |
| (1.6) `bar_style` | `"capsule"` | `"capsule"`, `"flat"`, `"thin"`, `"striped"` (moving stripes), `"glow"` or `"pips"` (a row of dots: `segments` of them, or 10) |
| (1.6) `buttons` | `"capsule"` | how choices look: `"capsule"`, `"box"`, `"outline"` or `"underline"` |
| (1.7) `board` | none | a grid of cells in place of `text`, `progress` and `choices`; see below |

```lua
tankyard.on("spawn", function(event)
  tankyard.hud("shop", {
    title = "Armory", text = "Pick one upgrade", style = "card", icon = "coin",
    enter = "pop", y = 0.7, choices = { "Repair", "Armor", "Later" },
  }, event.actor)
  -- Everyone sees who to chase: a glowing tag over the new tank that
  -- points the way while they are off screen.
  tankyard.hud("bounty", {
    text = "Bounty", style = "pill", icon = "skull", glow = true, point = true,
    pin = event.actor,
  })
end)
-- A warning in its own look: a slanted, striped panel with outlined
-- capitals and a striped bar.
tankyard.on("round_start", function()
  tankyard.hud("storm", {
    title = "Storm", text = "Shelter in the bunkers", style = "panel", y = 0.12,
    shape = "slant", fill = "stripes", border = "thick", lettering = "outline", caps = true,
    panel = "#181818", accent = "#ffc800", fill_color = "#ffc800", progress = 1,
    bar_style = "striped",
  })
end)
tankyard.on("choice", function(event)
  if event.name ~= "shop" then return end
  if event.value == 1 then tankyard.heal(event.actor, 200) end
  if event.value == 2 then tankyard.set_armor(event.actor, 1.25) end
end)
```

A menu shown to one player closes when they pick; one shown to everyone stays until the script changes it. A player picks at most twenty times a second. Widgets appear with their entrance, fade out when removed, and keep their place while their text changes. With **Reduced motion** on, players see them without movement.

Widget IDs use lowercase letters, digits and `_`. A pack may show 16 widgets to everyone and 16 to each player, and a player receives at most 64 from all packs. Bots have no screen, so widgets for bots are dropped. Widgets end with the round, with their player, or when the pack is disabled.

The server decides what each player sees and sends it at most ten times a second, only when it changes, so setting the same widget every tick costs nothing on the network. A pinned widget follows its tank or group on each player's screen by itself, so it needs no updates as the tank moves. Clients draw exactly what they receive. HUD widgets were introduced in 0.12, styles, icons and choices in 1.4, pins, choice layouts and glow in 1.5, shapes, borders, fills, lettering, title colors, bar looks and button looks in 1.6, and boards in 1.7. Tank Yard 2.1.1 uses protocol 35, the same as 2.1.0 (2.0 used 34); players and servers must use matching protocols.

#### Boards

(1.7) `board` turns a widget into a grid of cells: a control panel, a keypad, the front of a computer. The widget keeps its `title` and looks, and leaves out `text`, `progress` and `choices`.

```lua
tankyard.hud("panel", {
  title = "Reactor", style = "panel", x = 0.85, y = 0.5,
  board = {
    columns = 4, rows = 3, cell = 44, gap = 4,
    cells = {
      { kind = "digits", span = { 4, 1 }, text = string.format("%04d", heat) },
      { kind = "button", at = { 0, 1 }, text = "Vent" },
      { kind = "button", at = { 1, 1 }, text = "Boost", lit = boosting },
      { kind = "light", at = { 2, 1 }, text = "Hot", lit = heat > 900, color = "#ff3c28" },
      { kind = "meter", at = { 3, 1 }, span = { 1, 2 }, progress = heat / 1000 },
      { kind = "screen", at = { 0, 2 }, span = { 3, 1 }, text = "All systems normal" },
    },
  },
}, player)
```

| Board field | Default | Meaning |
| --- | --- | --- |
| `columns`, `rows` | `4`, `4` | the grid, 1 to 32 squares each way |
| `cell` | `40` | each square's side, 12 to 96, scaled with the interface |
| `gap` | `4` | space between squares, 0 to 24 |
| `cells` | none | up to 128 cells, drawn in order: where cells overlap, the later one draws on top and takes the click |

| Cell field | Default | Meaning |
| --- | --- | --- |
| `kind` | `"label"` | `"label"`, `"button"`, `"screen"` (dark glass with words), `"digits"` (seven-segment digits: 0 to 9, a minus sign, dots, colons and the letters segments can draw), `"light"` (a lamp with an optional caption) or `"meter"` (a bar; a tall one fills from the bottom up) |
| `at` | `{ 0, 0 }` | the column and row of its top-left square, from 0 |
| `span` | `{ 1, 1 }` | how many columns and rows it covers; it must stay inside the grid |
| `text` | none | up to 48 bytes: a label's, button's or screen's words, the digits, or a light's or meter's caption |
| `lit` | `false` | whether a button or light is lit |
| `progress` | `0` | how full a meter is, 0 to 1 |
| `color`, `fill` | the widget's | the cell's own color, and its background, as `"#rrggbb"` or `{ r, g, b }` |
| `size` | `0` | text size against what fits the cell, 0.25 to 2; 0 fits it |
| `align` | `"center"` | `"left"`, `"center"` or `"right"` |

A click on a button raises the pack's `choice` event with `value` set to the button's place in `cells`, counting from 1, and the board stays up. Labels, screens, digits, lights and meters can't be clicked. Set the board again to change what it shows: like any widget, it costs network only when it changes.

A coordinate pin can follow a moving map group:

```lua
tankyard.hud("lift", {
  text = "Board here", style = "pill", icon = "flag", point = true,
  pin = { x = 640, y = 480, group = 2 },
})
```

Here `x` and `y` are map coordinates before motion. The marker follows group 2,
including pushed bodies and paths, and hides while that group is hidden.
Menus shrink to fit small screens; picking a choice takes priority over controls
underneath it. Labels keep their own identity even when they overlap or move.

### Defining content

While loading, these declare content with the same fields as [data packs](#content-reference-tanks-maps-textures):

| Function | Declares |
| --- | --- |
| `tankyard.define_class(t)` | a tank (up to 1,024) |
| `tankyard.define_map(t)` | a map (up to 256) |
| `tankyard.texture(id, "art/skin.png")` | a texture (up to 16) |
| `tankyard.set_rules(t)` | the [rule profile](#rules) |
| `tankyard.mode_name(name)` | the mode's label |
| `tankyard.base_class(id)`, `tankyard.base_map(id)` | an editable copy of an existing definition (built in, or from a pack loaded earlier) |

A new tank joins the upgrade tree when its parent lists it, so redefine the parent too:

```lua
local parent = tankyard.base_class("spray")
table.insert(parent.upgrades, "sprinkler")
tankyard.define_class(parent)

local tank = tankyard.base_class("fan")
tank.id, tank.name = "sprinkler", "Sprinkler"
tank.parents, tank.upgrades = { "spray" }, {}
for index, barrel in tank.barrels do
  barrel.spread = 0.35 + index * 0.05
  barrel.pattern = "Spin"
end
tankyard.define_class(tank)
```

Definitions are validated like data packs. See `mods/sprinkler` for the full example.

### Menu, theme and wording

Packs can restyle the game itself, like a resource pack. These apply only on the player's own machine, to the menus and HUD, and are never sent to servers or other players.

```lua
tankyard.menu({
  title = "OVERCLOCK",                    -- replaces "Tank Yard"
  subtitle = "The rules change every 40 seconds",
  splashes = { "Pass the bomb!", "First to 5,000!" },  -- one per launch
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

| Theme color | Used for |
| --- | --- |
| `accent` | selections, outlines, switches and progress |
| `background` | behind menus |
| `surface` | panels and buttons |
| `line` | borders |
| `text`, `muted` | main and secondary text |
| `highlight` | headings, tags and splash text |

`menu.bosses` is an optional list of up to 16 boss ids whose cards should stay
available in the local Boss Rush roster. For example,
`"menu": { "bosses": ["sunderworm"] }` makes that encounter selectable every
visit. It does not change natural spawn odds or add it to the default lineup.
Enabled packs combine their lists; disabling a pack removes its additions unless
a boss is already selected in the saved lineup. This setting requires
Tank Yard 2.1.0 or later (introduced with protocol 35).

Data packs use the same fields as `"menu"`, `"theme"` and `"text"` objects in `mod.json`. Later packs win field by field, splashes add up (64 at most) and text replacements merge (256 at most). Unset theme colors keep the player's light or dark theme.

## Sandbox and limits

Each pack gets its own isolated Luau state, recreated every round. Scripts get the base functions, `string`, `table`, `math`, `bit`, `bit32`, `utf8`, `buffer` and `coroutine`. These are removed: `os`, `io`, `debug`, `ffi`, `jit`, `package`, `load`, `loadstring`, `loadfile`, `dofile`, `getfenv`, `setfenv`, `collectgarbage`, `newproxy` and `string.dump`. Only text source is accepted; bytecode never loads. `math.random` is seeded per round.

| Budget | Limit |
| --- | --- |
| Steps per event handler (loop iterations and calls) | 500,000 |
| Steps per pack per tick | 1,000,000 |
| Steps while loading | 20,000,000 |
| Memory per pack | 64 MiB |
| `string.rep` result | 1 MiB |
| Pattern matching subject | 64 KiB, with a quantifier cost limit |
| `table.sort` | 100,000 elements |
| Query results (`shapes`, `projectiles`) | 256 |
| Handlers per event / timers | 32 / 64 |

Luau checks budgets at every loop iteration and call, in compiled code too, so a runaway loop stops without disabling the compiler. `pcall`, `xpcall` and `coroutine.resume` can't catch a budget error.

Packs as a whole:

| Limit | Value |
| --- | --- |
| Enabled packs | 16 |
| Archive | 16 MiB, 256 files, 4 MiB per file; no links, encryption or size-header mismatches |
| `mod.json` and the combined data sent to players | 4 MiB each |
| Tanks / maps / textures | 1,024 / 256 / 16 |

## Workshop plugins

For a complete walkthrough, API reference, runnable example and troubleshooting, see the editor plugin guide. [Editor plugins](editor-plugins.md)

A workshop plugin adds tools and checks to the map editor. List its files under `editor` in `mod.json`:

```json
{
  "api_version": 2,
  "id": "my_tools",
  "name": "My Tools",
  "version": "1.0",
  "editor": ["tools.lua"]
}
```

A pack can have both `scripts` and `editor` files; the editor files never run in a match, and the match scripts never run in the editor. A pack with only `editor` files is editor-only: it never loads into a match and never marks one as modded. Enable plugins on **Mods**. They follow the same review process as other packs, and packs held for review need approval before their tools appear.

Editor files share the [sandbox](#sandbox-and-limits) and `require` of match scripts, but have none of the match API. They get one global, `editor`:

```lua
editor.tool {
  id = "row",                      -- unique in the pack, up to 48 characters
  name = "Row of blocks",          -- the button and heading
  description = "Blocks in a row from the pointer.",  -- optional, up to 200
  params = {
    { id = "count", name = "Blocks", type = "integer", min = 1, max = 20, default = 5 },
    { id = "gap", name = "Gap", type = "number", min = 0, max = 400, step = 10, default = 80 },
  },
  run = function(map, params, ctx)
    local x, y = ctx.pointer.x, ctx.pointer.y
    for i = 1, params.count do
      table.insert(map.blocks, {
        rect = editor.rect(map, x + (i - 1) * (40 + params.gap), y, 40, 40),
        style = "slate",
        group = 0,
      })
      ctx.select("blocks", #map.blocks)
    end
    ctx.say("Placed " .. params.count .. " blocks.")
    return map
  end,
}

editor.check {
  id = "no_bosses",
  name = "Boss calls",
  run = function(map)
    local found = {}
    for i, trigger in ipairs(map.triggers) do
      if trigger.action == "boss" then
        found[#found + 1] = { text = "This map calls a boss.", list = "triggers", index = i }
      end
    end
    return found
  end,
}
```

A tool's `run` receives a fresh copy of the map in the [map schema](#maps), its parameter values by `id`, and a context. It changes the map in place or returns a new one. The workshop runs the result through the same validation as an import before keeping it, as one undo step; if the run errors, runs out of budget or produces an invalid map, nothing changes and the Tools tab shows why.

| Parameter `type` | Fields | Value in `params` |
| --- | --- | --- |
| `number` (default) | `min`, `max`, `step`, `default` | a number |
| `integer` | `min`, `max`, `default` | a whole number |
| `toggle` | `default` | `true` or `false` |
| `choice` | `options` (1–24 names), `default` | the chosen name |
| `text` | `limit` (1–200), `default` | a string |

A tool has at most 12 parameters, and a pack at most 32 tools and 32 checks.

| Context | Meaning |
| --- | --- |
| `ctx.selection` | the selected objects, as `{ list = "blocks", index = 3 }` |
| `ctx.pointer` | `{ x = …, y = … }` in world units: the pointer, when Ctrl+R runs the tool over the canvas; otherwise where the canvas was last clicked, or the middle of the view |
| `ctx.grid` | the editor's grid size in world units |
| `ctx.view` | `{ x, y, w, h }`: the part of the arena the canvas shows, in world units |
| `ctx.select(list, index)` | select an object when the tool finishes; without a call, the selection stays |
| `ctx.say(text)` | a message for the editor, up to 8 per run |

Lists are `walls`, `blocks`, `zones`, `decor`, `triggers`, `spawn_regions`, `team_bases` and `objective_positions`, and indexes start at 1. Map rectangles are fractions of the arena; `editor.rect(map, x, y, w, h)` and `editor.point(map, x, y)` convert from world units, and `editor.world(map, r)` converts back. `editor.free_group(map)`, `editor.free_channel(map)` and `editor.free_counter(map)` return the lowest group, signal channel or counter the map doesn't use yet, or `nil` when all are taken, so tools that build logic don't collide with what is there. `print` output appears under **Last run** in the Tools tab.

A check's `run` receives the map and returns a list of findings: strings, or tables with `text` and optionally `list` and `index` to select an object when the note is clicked. The first 64 appear as amber notes in **Check**, prefixed with the pack's name. Checks advise; they never block a playtest or export.

Each tool run, or a pack's checks together, has a 20,000,000-step budget. A plugin that runs out stops until **Reload plugins**, which also picks up edited files. A plugin's Luau state otherwise lasts until the editor reloads it. `tankyard-tools validate-mods` loads editor plugins too and reports their errors. See `mods/workshop_toolkit` and `mods/logic_kit` for complete packs.

## Data packs and JSON hooks

A data pack is a `mod.json` with `api_version` 1. Definitions replace an existing ID or add a new one. Dependencies load first, then packs in the order given; later definitions win.

```json
{
  "api_version": 1,
  "id": "vampire",
  "name": "Vampire",
  "version": "1.0",
  "hooks": [
    { "id": "heal_after_kill", "event": "kill", "actions": [{ "type": "heal", "amount": 15.0 }] }
  ]
}
```

### Rules

`rules` replaces the whole rule profile; omitted fields take the defaults below. Packs without `rules` keep the server's configured physics.

| Field | Default | Range |
| --- | --- | --- |
| `movement_multiplier` | 1.0 | 0.1–10 |
| `acceleration` | 8.0 | 0.1–30 |
| `coast` | 6.0 | 0.1–30 (how quickly tanks slow down) |
| `damage_multiplier` | 1.0 | 0–10 |
| `health_multiplier` | 1.0 | 0.1–10 |
| `regen_multiplier` | 1.0 | 0–10 |
| `xp_multiplier`, `score_multiplier` | 1.0 | 0–10 |
| `respawn_seconds` | 3.0 | 0.5–60 |

Clients receive the same rules the server simulates with, so movement prediction stays exact.

### Hooks

```json
{
  "id": "comeback",
  "event": "damage",
  "target": "actor",
  "cooldown_ticks": 600,
  "conditions": [
    { "type": "health_below", "fraction": 0.25 },
    { "type": "not", "condition": { "type": "is_bot", "value": true } }
  ],
  "actions": [
    { "type": "set_shield", "seconds": 2 },
    { "type": "hud", "id": "comeback", "widget": { "text": "Second wind!", "color": [255, 176, 0] } }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `id` | lowercase letters, digits and `_` |
| `event` | any [event](#events) |
| `target` | `actor` (default), `all_players` or `alive_players` |
| `every_ticks` | run on every nth matching event (default 1) |
| `cooldown_ticks` | per-tank cooldown (default 0) |
| `conditions` | all must hold, for each target tank |
| `actions` | applied in order, validated together |

Conditions:

| `type` | Fields | Holds when |
| --- | --- | --- |
| `health_below` | `fraction` | health / max health is below `fraction` |
| `level_at_least` | `level` | |
| `class_is` | `class` | |
| `team_is` | `team` | |
| `is_bot` | `value` | the tank is (`true`) or isn't (`false`) a bot |
| `counter_at_least` | `name`, `value` | the tank's counter has reached `value` |
| `any` | `conditions` | at least one holds |
| `not` | `condition` | the condition doesn't hold |

Actions take the same bounds as the [Lua commands](#changing-the-match):

| `type` | Fields |
| --- | --- |
| `heal`, `damage` | `amount` |
| `award` | `xp`, `score` |
| `set_class` | `class` |
| `set_team` | `team` |
| `set_level` | `level` |
| `raise_stat` | `stat` (0–7) |
| `set_scales` | `speed`, `reload`, `damage` |
| `set_armor` | `multiplier` |
| `set_shield`, `set_protection` | `seconds` |
| `impulse`, `set_velocity` | `velocity: [x, y]` |
| `teleport` | `position: [x, y]`, as fractions of the arena (0–1) |
| `spawn_target` | `position: [x, y]` (fractions), `sides`, `health`, `xp` |
| `fire` | `speed`, `damage`, `radius`, `lifetime`, `angle` |
| `control` | `movement: [x, y]`, `aim`, `fire`, `ability` |
| `counter` | `name`, `add`: adds to the target tank's counter (−1e9 to 1e9) |
| `kill`, `clear_projectiles` | none |
| `set_score` | `score` |
| `set_team_score` | `team`, `score` |
| `spawn_boss` | `boss` |
| `announce` | `message` |
| `finish_round` | `team` (optional), `reason` |
| `hud` | `id`, and `widget` (fields as [HUD widgets](#hud-widgets), colors as `[r, g, b]`); leave out `widget` to remove it. Shown to the event's tank, or to everyone for round events |
| `map_signal` | `channel` (1–32) |
| `map_counter` | `counter` (1–8), `value`, `add` (`true` adds, otherwise sets) |
| `map_group` | `group` (1–32), `shown` |
| `map_reset` | (0.16) `group` (0–32; 0 resets every trigger) |
| `map_motion` | (1.0) `group` (1–32), `action` (`place`, `turn`, `fade`, or (1.1) `scale`), `value: [x, y]` (the offset, or degrees or percent first, as in the script commands above), `seconds` (0–600) and `easing` (`{ "curve": "bounce", "mode": "out" }`) |
| `map_tint` | (1.1) `group` (1–32), `color: [r, g, b]`, `percent` (0–100), `seconds` and `easing` |
| `map_animate` | (1.1) `group` (1–32), `style` (`bob`, `sway`, `throb`, `shake` or `orbit`), `size`, `speed` (0–12), `seconds` and `easing`, bounded as in `animate_map_group` |
| `map_stop` | (1.1) `group` (1–32) |
| `shake_view` | (1.1) `percent` (1–100), `seconds` (0.1–10): shakes the target tank's view, or every player's for round events |
| `play_sound` | (1.1) `sound`, `at: [x, y]` as fractions of the arena (0–1), `percent` (1–100, default 100) |
| `map_path` | (1.3) `group` (1–32), `path` (1–32) |
| `map_light` | (1.3) `darkness` (percent, 0–95), `color: [r, g, b]`, `seconds` (0–600) and `easing` |
| `map_shove` | (1.3) `group` (1–32), `velocity: [x, y]` (each ±2,000 units a second) |
| `fling` | (1.3) `velocity: [x, y]` (each ±3,000 units a second): throws the target tank |

A pack has at most 64 hooks, 16 conditions and 16 actions per hook, conditions nested four deep, 10,000 condition checks per tick, 4,096 counters and 16,384 cooldown entries. Hooks don't trigger other hooks. A pack whose batch fails validation is disabled without applying it. Counters and cooldowns reset each round.

## Content reference: tanks, maps, textures

### Tanks

Tank definitions use the same schema as [`assets/content/classes.json`](../assets/content/classes.json). The quickest way to make one is to copy a built-in tank with `tankyard.base_class` or from that file.

| Field | Meaning |
| --- | --- |
| `id`, `name`, `description` | identity and the upgrade menu text |
| `tier`, `min_level` | tiers unlock at levels 1, 15, 30, 45 and 55 |
| `parents`, `upgrades` | the upgrade tree; both ends of each edge must agree |
| `multipliers` | eight numbers: health, regeneration, body damage, bullet speed, penetration, bullet damage, reload (higher fires faster) and movement |
| `hull_sides` | 0 for a circle, 3–16 for a polygon |
| `body_style` | `auto`, `circle`, `smasher`, `landmine` or `spike` |
| `body_scale` | scales the model and collision size (default 1) |
| `body_texture` | a texture ID |
| `barrels` | the guns (below) |
| `drone_limit` | drones and minions alive at once (default 8) |
| `ability` | `None`, `Dash`, `Shield`, `Heal`, `Nova`, `Recall`, `Cloak`, `Overdrive`, `Blink`, `Fortify`, `Magnet`, `Parry`, `CounterParry`, `VolleyParry`, `Stasis` or `Pocket` |
| `ability_cooldown`, `passive_regen` | seconds; health per second |
| `strengths`, `weaknesses` | text for the upgrade menu |
| `ai_range` | how far away bots using this tank like to fight |

Each skill has seven levels; a level-45 tank has earned 33 points and a level-55 tank 38. Built-in tanks must each play differently; packs may add alternate versions.

Barrels use tank-local coordinates (x forward, y to the side). Angles are radians, lengths and speeds are arena units, and times are seconds.

| Field | Meaning |
| --- | --- |
| `offset`, `angle` | mount position and direction |
| `length`, `width`, `profile` | the drawn barrel: `auto`, `rectangle`, `trapezoid`, `trap` or `rocket` |
| `cooldown`, `delay`, `recoil` | seconds between shots, initial delay, push-back |
| `pattern` | `Normal`, `Alternating`, `Burst`, `Charge`, `Auto` or `Spin` |
| `projectile` | `Bullet`, `Shell`, `Piercing`, `Scatter`, `Drone`, `Trap`, `Mine`, `Explosive`, `Fragment`, `Homing`, `Barrier`, `Turret` or `Minion` |
| `speed`, `damage`, `radius`, `lifetime`, `penetration` | the projectile; trap launch distance uses `speed` |
| `count`, `spread` | projectiles per shot and their spread in radians |
| `ricochets` | wall bounces |
| `target_arc` | limits automatic aiming to a cone (radians) |

Barrels at the same spot with the same size share one drawn cannon.

**Minions and drones.** A barrel with `"projectile": "Minion"` launches a small tank with its own gun: a real, server-simulated projectile, not a second player. Minions hunt visible targets on their own; holding fire sends them circling the cursor, shooting at it, and right-click pulls them back. `Drone` and `Minion` share one pool per tank, sized by `drone_limit` (six suits a Factory-style tank, eight a standard controller). A destroyed minion frees its slot, and its barrel refills it. The barrel's `cooldown`, `speed`, `damage`, `radius` and `penetration` still apply.

### Maps

Map definitions use the schema of [`assets/content/maps.json`](../assets/content/maps.json). Coordinates are fractions of the arena (0–1).

| Field | Meaning |
| --- | --- |
| `id`, `name`, `description` | identity; custom maps appear in the map picker |
| `width`, `height`, `capacity` | suggested size and players (a server's own settings win) |
| `recommended_modes` | such as `["ffa", "two_teams", "four_teams"]` |
| `walls` | `[x, y, w, h]` rectangles |
| `procedural` | `"None"`, `"Maze"` or `"Dense"`: seeded layouts generated on the server |
| `objective_positions` | `[x, y]` points for objective modes |
| `spawn_regions` | `{ "team": 0–4, "rect": [x, y, w, h] }`; team 0 is everyone |
| `team_bases` | `{ "team": 1–4, "position": [x, y] }` |
| `background` | a background name, such as `"teal"` |

The [Arena Workshop](map-editor.md) creates these definitions without writing code. Optional extended fields use protocol 24; conveyors, portals and the trigger fields marked below need protocol 25:

- `style`: `map`, `slate`, `garden`, `circuit`, `bastion`, `relay`, `orrery` or `sand`.
- `blocks`: `{ "rect": [x,y,w,h], "style": "slate", "color": [r,g,b], "group": 0 }`. Color is optional; these are solid, axis-aligned rectangles.
- `zones`: `{ "kind": "speed", "rect": [x,y,w,h], "strength": 1, "group": 0, "angle": 0, "exit": [x,y] }`. Kinds are `speed`, `mud`, `repair`, `hazard`, `shelter`, (protocol 25) `conveyor` and `portal`, (1.1) `launch`, and (1.3) `ice` and `pull` (a gravity well); strength is 0.25–2. A conveyor pushes along `angle` in degrees (0 right, 90 down) at `150 × strength` units a second, and a launch pad flings tanks along `angle` at `900 × strength`. A portal sends tanks to `exit`, a map-relative point that must be open floor. See the [pad formulas](map-editor.md#objects).
- `decor`: `{ "kind": "tile", "rect": [x,y,w,h], "color": [120,160,190], "alpha": 255, "rotation": 0, "pulse": 0, "spin": 0, "flicker": 0, "above": false, "text": "", "effect": null, "group": 0 }`. Kinds: `tile`, `stripes`, `light`, `ring`, `arrow`, `label`, `emitter`, and (1.3) `spotlight`, `beam`, `grid`, `disc`, `triangle` and `hexagon`. Rotation/spin are degrees; pulse is cycles/second. (1.0) `flicker`, 0–1, makes a `light`, `spotlight` or `beam` waver like a flame or a failing lamp. (1.3) `react` makes decor respond to the player's own tank: `fade` (clears as it comes near, for roofs), `appear`, `grow` or `glow`, within `reach` world units (10–2,000; 220 when left out). Text has at most 64 characters.
- `triggers`: `{ "when": "every", "action": "toggle", "rect": [x,y,w,h], "group": 1, "seconds": 10, "offset": 0, "duration": 4, "once": false }`. Timing kinds: `every`, `enter`, `start`, (protocol 25) `exit`, `signal`, `counter`, (0.15.1) `occupied` (every `seconds`, at least 0.25, while `threshold` tanks, 1–64, are inside), `cleared` (the last noticed tank leaves or dies) and `destroyed` (a noticed tank dies inside), and (0.16) `changed` (counter `source`, 1–255, changes value), `shown` and `hidden` (group `source`, 1–32, is shown or hidden during the round, by anything) and `defeated` (a boss is defeated; `source` 0 for any boss, or a boss's place in the list under `spawn_boss` counting from 1), and (1.0) `arrived` (a motion of group `source`, 1–32, ends). Actions: `toggle`, `show`, `hide`, `announce` (add `text`), `boss` (add `boss`), (protocol 25) `signal` (channel `group`), `count` (adds `amount` to counter `group`) and `shapes` (`amount` shapes, 1–24, inside `rect`), and (0.15.1) `set` (puts counter `group` at `amount`), `random` (signals one of `amount` channels, at least 2, starting at `group`; see `pick` below), `heal` and `hurt` (`amount` percent of health, 1–100) and `award` (`amount` points, 1–100,000) for the noticed tanks inside `rect`, `calculate` (changes counter `group` with `math` and `amount`, or with counter `operand`) and `script` (raises the [`map_event`](#map-logic) script event named `text`: lowercase letters, digits and `_`, starting with a letter, up to 32), and (0.16) `sequence` (signals the next of `amount` channels starting at `group` each time, then starts over) and `reset` (makes group `group`'s triggers start afresh, or every trigger for 0, like `reset_map_triggers`), (1.0) the [motions](#motions) `move`, `place`, `turn` and `fade`, (1.1) the [effects](#effects) `tint`, `scale`, `animate`, `stop`, `shake` and `sound`, and (1.3) the [physics and lighting](#physics-and-lighting) actions `shove`, `fling`, `blast`, `zoom`, `light` and `path`. (1.3) `touch` and `release` act when a block of group `source` starts to overlap `rect`, or the last one stops. Protocol 25 fields, all optional:
  - `member`: the trigger's own group; it is off while that group is hidden.
  - `who` (`anyone`, `players`, `bots`, and in 1.4 `cause`) and `team` (0–4): which tanks an area trigger notices, or which tanks a `heal`, `hurt` or `award` action reaches. `cause` reaches only the tank that set the trigger off: the one that crossed an area, or picked a choice, behind the signal it heard; area triggers with it watch for any tank.
  - `source`: the signal channel or counter a trigger waits for, 1–255 each (1.7; channels 1–32 and counters 1–8 before). `threshold`: the counter value it waits for.
  - `chance`: 1–100 percent, default 100. `delay`: seconds between firing and acting, 0–3600.
  - (0.15.1) `only_if`: a gate checked when the trigger fires; it acts only if the gate holds. `{ "kind": "counter", "index": 1, "compare": "at_least", "value": 3, "against": 0 }` compares counter `index` with `value`, or with counter `against` when that is 1–255. `compare` is `equal`, `not_equal`, `below`, `above`, `at_most` or `at_least`. `{ "kind": "shown", "index": 4 }` and `{ "kind": "hidden", "index": 4 }` test a group. (0.16) `{ "kind": "inside", "compare": "at_least", "value": 2 }` counts the noticed tanks inside the trigger's `rect` (with its `who` and `team`), and `{ "kind": "time", "compare": "at_least", "value": 300 }` reads whole seconds since the round started; both compare like a counter, with `value` or counter `against`. (1.5.1) Neither needs `index`; Tank Yard 1.5.0 and earlier want `"index": 0` on them. (2.0.5) `{ "kind": "chance", "value": 25 }` holds 25% of the time, rolled once each time the trigger tries to act, or with counter `against`, that counter's value as the percent.
  - (0.16) `also_if`: up to three more conditions, shaped like `only_if`, which they need. `gate` says how all of them combine: `all` (AND, the default), `any` (OR), `not_all` (NAND), `none_of` (NOR) or `one` (XOR, exactly one holds). `otherwise`: a signal channel, 1–255, sent instead when the gate is shut; 0 or left out sends nothing.
  - (0.15.1) `math`: how `calculate` changes its counter: `add`, `subtract`, `multiply`, `divide` (rounds down), `remainder` (never negative), `lowest`, `highest`, `copy`, `random` (0 up to one less than the other value) or `flip` (0 becomes 1, anything else 0), and (1.7) `and`, `or` and `xor` (bit by bit), `shift_left` and `shift_right` (doubling or halving, rounding down, that many times) and `power`. Dividing by zero changes nothing, and so does a negative power. `operand`: use counter 1–255 instead of `amount`. (1.7) `target_offset` and `operand_offset`: add that counter's value to the number of counter `group` or counter `operand`, for lists of counters; a number outside 1–255 does nothing.
  - (2.0.5) `pick`: how `random` picks: `any` (the default), `fresh` (never the channel it sent last time), `shuffle` (every channel once, in a random order, before any repeats; `reset` starts the round over) or `weighted` (by `weights`, one number from 0 to 100 a channel, in order, for up to 32 channels, at least one above 0). `picks`: send on that many different channels at once, up to one fewer than `amount`.

  Trigger fields added in 0.15.1, 0.16, 1.0 and 1.1 are left out of exported maps when unused. A map that uses them needs a server (or offline game) from that version; older ones refuse the map rather than run it wrongly. Decor and particles in 1.0 always carry their new fields, so maps from 1.0 need 1.0.

  Counters 1–255 start at 0 each round and stay within ±1,073,741,823 (8 counters within ±99,999 before 1.7). Announcements and decor labels replace `{1}`–`{255}` with counter values, in the [counter formats](map-editor.md#counters): `{3:,}` with commas, `{3:04}` padded, `{3:b}` and `{3:x}` in binary and hex, and `{3|2|1:4}` as parts of one long number. The [logic guide](map-editor.md#groups-and-logic) explains schedules, entry edges, chains and restoration.
- `hidden_groups`: groups 1–32 hidden at round start. Group 0 is always visible. Blocks, zones and decor sharing a group change together.
- (1.3) `group_names`: `{ "group": 3, "name": "Crate" }`, up to 24 characters, shown in the workshop.
- (1.3) `bodies`, `paths` and `ambient`: see [Physics and lighting](#physics-and-lighting).

#### Motions

Protocol 26 (Tank Yard 1.0) adds triggers that animate a group, in the manner of Godot's tweens. Group `group`'s blocks, pads, decor and trigger areas move together, and moving cover pushes tanks, shapes and bosses ahead of it. Turns and fades change only how decor looks.

- `move` glides the group by `travel` from wherever it is, and `place` glides it to `travel` from where the map places it (`[0, 0]` brings it home). `travel` is `[x, y]` in fractions of the arena's width and height, each within ±2; a group never ends up more than four arenas away.
- `turn` turns the group's decor `amount` degrees further (±3,600), and `fade` fades it to `amount` percent opacity (0–100).
- `tween` is how many seconds the motion takes, 0–600; 0 is instant. A move or place may average at most one arena width or height a second.
- `easing` shapes the motion: `{ "curve": "sine", "mode": "in_out" }` is the default. Curves are `linear`, `sine`, `quad`, `cubic`, `quart`, `quint`, `expo`, `circ`, `back` (pulls back or overshoots a little), `elastic` (springs past the end) and `bounce`. Modes are `in` (starts slowly), `out` (ends slowly), `in_out` and `out_in`.
- `duration`, when above 0, plays the motion back that many seconds after it ends: a door that shuts again, a lift that comes back down. If a motion that goes back plays again before the group is home, the wait starts over and the group still returns to where it rested before it set off.

A new motion on a group and channel takes over from the one playing, starting where the group is now. An `arrived` trigger watching group `source` acts each time one of the group's motions ends, the trip back included, but not for a motion another took over from or one with nowhere to go; chain motions or open a door once a lift arrives with it. Motions reset each round.

```json
{ "when": "enter", "action": "place", "rect": [0.45, 0.4, 0.1, 0.2], "group": 3,
  "travel": [0, -0.06], "tween": 0.5, "easing": { "curve": "cubic", "mode": "out" },
  "duration": 4, "seconds": 1 }
```

This is the workshop's Door piece, with group 3 as the door. The [motions tutorial](motions-tutorial.md) builds a gatehouse around one in the workshop, with moving cover, chained motions, lights, particles and effects.

#### Effects

Tank Yard 1.1.0 (protocol 28) adds triggers that change how a group looks and how the arena feels. `tint`, `scale` and `animate` play like motions, with `tween`, `easing` and `duration`, and change only decor and emitters.

- `tint` mixes `color` (`[r, g, b]`) into the group's decor, lights and particles at `amount` percent (0–100); 0 removes the tint.
- `scale` draws the group's decor and emitters at `amount` percent of their placed size (0–400).
- `animate` plays `style` on the group's decor: `bob`, `sway`, `throb`, `shake` or `orbit`. `amount` is its size: world units for `bob`, `shake` and `orbit` (0–400), degrees for `sway` (0–180) and percent for `throb` (0–100); 0 settles it. `speed` is cycles a second (0–12); 0 or left out uses the style's own pace (1, or 8 for `shake` and 0.5 for `orbit`). The five styles play together, and changing the speed keeps the animation's place in its cycle.
- `stop` halts group `group`'s motions where they are, freezes its animations and cancels its trip back. What it halts never arrives.
- `shake` shakes the views of the noticed tanks inside `rect` at `amount` percent (1–100) for `tween` seconds (0.1–10).
- `sound` plays `sound` at the middle of `rect`, at `amount` percent volume (1–100). Sounds are `chime`, `bell`, `alarm`, `switch`, `whoosh`, `rumble`, `fanfare` and `thump`, and (2.0.5) `coin`, `pop`, `beep`, `click`, `tick`, `laser`, `power_up`, `power_down`, `buzzer`, `success`, `door`, `explosion`, `teleport`, `magic`, `splash`, `gong`, `thunder`, `countdown`, `drum` and `horn`. Players hear it by distance, at their Effects volume. (2.0.5) Optional settings: `pitch` in semitones (−12 to 12), `vary` (how far each play wanders either way, in tenths of a semitone, 0–60), `repeat` (plays, 1–8), `gap` (seconds between plays, 0–3) and `range`: `near` (the default), `far` (carries about two and a half times as far), `everywhere` (full volume for every player) or `cause` (only the player whose tank set it off). Players on older versions hear the nearest of the first eight sounds, once, nearby.

```json
{ "when": "start", "action": "animate", "rect": [0.45, 0.4, 0.04, 0.05], "group": 2,
  "style": "orbit", "amount": 40, "speed": 0.25, "tween": 1, "seconds": 0 }
```

This is the workshop's Orbit piece: group 2's decor eases into a slow circle 40 units wide as the round starts.

#### Physics and lighting

Tank Yard 1.3.0 (protocol 29) adds pushable bodies, paths, darkness and triggers that throw tanks.

`bodies` makes a group's blocks one solid body that tanks push by driving into it; its decor, pads and trigger areas ride along. One body per group:

```json
{ "group": 2, "axis": "along", "reach": [0, 0.2, 0, 0], "weight": 1,
  "spring": 0.12, "friction": 8, "bounce": 0, "shots": false }
```

- `axis`: `free` (any way), `across` (left and right) or `along` (up and down).
- `reach`: how far it can slide from its place, left, up, right and down, as fractions of the arena (0–1 each).
- `weight` (0.25–8): one tank pushes weight 1 at its own speed. `spring` (0–1, arena widths a second) slides it home once let go; 0 leaves it where it was pushed. `friction` (0.5–20) slows it after a shove, `bounce` (0–0.9) keeps some speed when it hits its reach or other cover, and `shots` lets bullets push it.

A body stops at other cover and never crushes a tank. Players receive bodies' positions and speeds in each snapshot. Bodies don't count against connectivity, and neither do groups that triggers hide or move, since they can be pushed or opened.

`paths` are custom animations a `path` trigger plays on a group:

```json
{ "name": "Ferry", "repeat": "bounce", "points": [
  { "at": [0.16, 0], "seconds": 2.5, "pause": 0.8, "turn": 0,
    "easing": { "curve": "sine", "mode": "in_out" } },
  { "at": [0.16, 0.12], "seconds": 1.5, "pause": 0.8 } ] }
```

`repeat` is `once`, `loop` (on from the last point to the first) or `bounce` (there and back). Each point's `at` is an offset from where the map places the group, as fractions of the arena (±2), reached in `seconds` (0–600) along `easing`, then held for `pause` seconds; `turn` turns the group's decor that many degrees on the way. Legs faster than one arena width a second are slowed to it. A map has up to 32 paths of up to 64 points. A group on a path arrives (for `arrived` triggers) only at its end; `move`, `place` or `stop` on the group takes it off.

`ambient` darkens the arena: `{ "darkness": 0.7, "color": [12, 16, 44] }`, with `darkness` 0–0.95 and the color the dark leans toward. Lights, spotlights, beams, hazard, portal, launch and repair pads, tanks and shots light it.

The 1.3 trigger actions:

- `shove` pushes body `group` along `angle` (degrees, 0 right, 90 down) at `amount` world units a second (1–2,000).
- `fling` throws the noticed tanks inside `rect` along `angle` at `amount` (1–3,000); they coast like off a launch pad.
- `blast` throws tanks inside `rect` away from its middle at `amount`, or pulls them in with a negative `amount` (±3,000).
- `zoom` sets the view of the noticed tanks inside to `amount` percent (40–250) for `tween` seconds (0.1–60), then eases back. (1.7) `hold: true` keeps it until the next zoom instead, `ease` sets the seconds the view takes to change (0.1–10; 0 or left out takes half a second) along `easing`, and `focus` turns the camera that many percent (0–100) toward the middle of `rect`.
- `light` changes the darkness to `amount` percent (0–95) tinted `color`, over `tween` along `easing`; `duration` returns it to what it was that many seconds later.
- `path` sends group `group` along path `amount` (1–32).

```json
{ "when": "touch", "action": "hide", "rect": [0.78, 0.13, 0.06, 0.09],
  "source": 3, "group": 4 }
```

This is the crate plate from the bundled [Night Works](../mods/night_works) map: the gate, group 4, hides while any block of the crate, group 3, sits on the plate. A `release` trigger on the same `rect` shows it again. The [physics tutorial](physics-tutorial.md) builds the whole map in the workshop.

Players receive maps without their `triggers` and `paths`; the server runs them and sends their effects in snapshots.

Emitters' `effect` is an optional particle profile. Defaults are:

```json
{
  "shape": "dot", "rate": 8, "lifetime": 1.6,
  "speed": 60, "direction": 270, "spread": 50,
  "gravity": 0, "drag": 0.6, "size": 9, "end_size": 3,
  "color": [255,200,90], "end_color": [230,90,60],
  "fade": true, "spin": 0,
  "glow": false, "wobble": 0, "twinkle": 0, "area": "fill"
}
```

Particle shapes are `dot`, `square`, `spark`, `ring`, `triangle`, and (1.0) `soft` (a dot with a soft edge, for smoke, mist and glows) and `star`. (1.0) `glow` adds particles' light to what lies beneath them, like embers; `wobble` drifts them sideways by up to that many world units (0–200), for smoke and fireflies; `twinkle` (0–1) makes each one flicker; and `area` says where they start: anywhere in the rectangle (`fill`), in the ellipse inside it (`round`), along its outline (`edge`) or at its center (`point`). Rate is 0.2–60/s, lifetime 0.1–8 s, speed 0–800, spread 0–360°, gravity ±800, drag 0–8, initial size 1–120, final size 0–120, and spin ±1080°/s. `rate × lifetime` must not exceed 240. Colors are three bytes; direction 0° points right and 90° down.

Maps allow 2,048 walls/blocks combined, 512 zones, 4,096 decor items (48 emitters), 4,096 triggers (2,048 decor items and 512 triggers before 1.7), 32 paths, 32 bodies, 32 spawn regions, 4 team bases and 32 objectives. Geometry validation also has a catalog-wide work budget. Dimensions are 900–16,000 by 700–16,000 world units. Every rectangle must fit inside `[0,1]²`. Validation checks starting collision/connectivity, finite values and count limits before accepting a pack.

#### UI

Tank Yard 1.4.0 (protocol 30) adds `ui` and `hide_ui` actions. A `ui` trigger shows its `ui` widget in slot `group` (1–32 in 1.5; 1–16 in 1.4); a `hide_ui` trigger takes slot `group` away (0 for every slot) from the players its `ui.to` names, or from everyone without a `ui`:

```json
{ "when": "enter", "action": "ui", "rect": [0.45, 0.4, 0.1, 0.1], "group": 3, "seconds": 2,
  "duration": 0,
  "ui": {
    "widget": { "title": "Workshop", "text": "Pick an upgrade, {name}", "y": 0.7,
                "style": "panel", "icon": "bolt", "enter": "slide",
                "choices": ["Repair", "Points", "Leave"] },
    "to": "cause", "channels": [1, 2, 0] } }
```

`widget` takes the [HUD widget](#hud-widgets) fields; its `text` and `title` fill in `{1}`–`{255}` (counters, in any [counter format](map-editor.md#counters)), `{left}` (seconds before it goes), `{time}` (the round's clock), `{name}` and `{score}` (the viewer's), and `{cause}` (the tank that set it off; empty when there is none). `to` is `everyone` (the default; later arrivals see it too), `inside` (players the trigger notices in its `rect`) or `cause` (the player that set it off). `bar` (1–255) fills the bar from that counter out of `bar_max` (1–1,073,741,823); `timer: true` empties it as `duration` runs out instead. Each choice signals its channel in `channels` (0 for none) with the picker as the tank that set it off, and the menu closes for them unless `keep` is true. `duration` above 0 takes the widget away after that many seconds. Bots never see widgets.

(1.5) `pin` floats the widget over the arena: `area` centers it over the middle of the trigger's `rect`, like a sign, following the trigger's group motion, and `cause` floats it just above the tank that set the trigger off and follows it, like a name tag, for everyone it shows to. A `cause` widget shows nothing when no tank set the trigger off (timers have none) and goes when that tank leaves the game. Set the widget's `point: true` to keep it near the edge of the screen with an arrow while what it floats over is out of view. Map widgets pin with the trigger's `pin`, never the widget's own. Maps save `pin`, `point`, `layout` and `glow` only when they're used, so maps that don't use them still open in 1.4.0:

```json
{ "when": "start", "action": "ui", "rect": [0.03, 0.3, 0.12, 0.08], "group": 4,
  "ui": {
    "widget": { "title": "Armory", "text": "Pick a loadout", "style": "card", "icon": "crown",
                "glow": true, "layout": "grid",
                "choices": ["Twin", "Sniper", "Machine", "Flank", "Smash", "Trap"] },
    "pin": "area", "channels": [1, 2, 3, 4, 5, 6], "keep": true } }
```

(1.7) A map widget's `board` takes the [board](#boards) fields, and its cells three more: a button's `channel` (1–255, 0 for none) is the signal a click sends, with the player who clicked as the tank that set it off; `counter` (1–255) with `bit` (1–30, or 0 for the whole counter) or `group` (1–32, lit while shown) lights a button or light; and a meter fills from `counter` out of `max` (1–1,073,741,823, default 100). The server works out `lit` and `progress` from these, and cells fill in [counter fields](map-editor.md#counters) like widget text. A board stays up when a button is clicked. Maps save cell fields only when they're used:

```json
{ "when": "start", "action": "ui", "rect": [0.45, 0.45, 0.1, 0.1], "group": 2,
  "ui": { "widget": { "title": "Vault", "style": "panel", "x": 0.85, "y": 0.5,
    "board": { "columns": 3, "rows": 2, "cells": [
      { "kind": "digits", "span": [3, 1], "text": "{1:04}" },
      { "kind": "button", "at": [0, 1], "text": "+1", "channel": 5 },
      { "kind": "button", "at": [1, 1], "text": "Open", "channel": 6 },
      { "kind": "light", "at": [2, 1], "counter": 2, "bit": 1 } ] } } } }
```

Boss IDs are `sentinel`, `prism`, `foundry`, `tempest`, `bastion`, `eclipse`, `guardian`, `summoner`, `defender`, `fallen_overlord`, `fallen_booster`, `siege_engine`, `skirmisher`, `trapper`, `railgunner`. Boss triggers use these IDs. Scripts' `spawn_boss` also accepts the existing display names.

### Textures and models

Textures are body skins. A texture entry has an `id` and a relative `file`, and a tank uses it through `body_texture`. The texture's right side faces where the tank aims. Paths can't leave the pack, even through links. Sources up to 4096 pixels on a side are shrunk to fit 256 × 256, keeping their shape. The server decodes the art and sends pixels, so players need nothing installed. SVGs must be self-contained: image references to local filesystem paths are ignored, including references inside embedded SVGs.

| Kind | Formats |
| --- | --- |
| Images | PNG, JPEG, GIF (first frame), BMP, TGA, WebP, ICO, ICNS |
| Vector art | SVG (rendered crisp at texture size; scripts and external files are ignored) |
| Editors | Aseprite (`.ase`, `.aseprite`, first frame), Photoshop (`.psd`, flattened), Krita (`.kra`, merged image) |
| 3D models | glTF/GLB, OBJ (with `.mtl` colors), STL, PLY (with vertex colors) |

Models render top-down into a sprite with flat lighting and the game's dark outline, so they match the built-in tanks. Blender, Maya, 3ds Max, SketchUp, MagicaVoxel, Tinkercad and others work through those exports. glTF and OBJ treat +Y as up and +Z as forward; STL treats +Z as up and +X as forward. Prefer `.glb` or embedded glTF buffers: a separate `.bin` works, but the safety review flags every `.bin` for a manual look. Export glTF instead of FBX or `.blend`.

## Recipes

### A custom mode with a round clock

```lua
--!strict
tankyard.mode_name("King of the Hill")
local HILL_RADIUS, ROUND_SECONDS = 260, 240
local held: { [number]: number } = {}

tankyard.on("round_start", function() table.clear(held) end)

tankyard.on("tick", function(event)
  local arena = tankyard.arena()
  local cx, cy = arena.width / 2, arena.height / 2
  for _, tank in tankyard.tanks() do
    if tank.alive and tankyard.distance(tank, { x = cx, y = cy }) < HILL_RADIUS then
      held[tank.id] = (held[tank.id] or 0) + 1
      if held[tank.id] % 60 == 0 then tankyard.award(tank.id, 0, 10) end
    end
  end
  local left = math.max(0, ROUND_SECONDS - event.tick // 60)
  if event.tick % 60 == 0 then
    tankyard.hud("clock", { text = `{left // 60}:{string.format("%02d", left % 60)}`, y = 0.06, size = 24 })
  end
  if left == 0 and not arena.finished then
    tankyard.finish_round(0, "Time's up")
  end
end)
```

### Bots that chase the leader

```lua
tankyard.on("tick", function()
  local leader
  for _, tank in tankyard.players() do
    if tank.alive and (not leader or tank.score > leader.score) then leader = tank end
  end
  if not leader then return end
  for _, bot in tankyard.tanks() do
    if bot.bot and bot.alive then
      local dx, dy = leader.x - bot.x, leader.y - bot.y
      local length = math.max(1, math.sqrt(dx * dx + dy * dy))
      tankyard.control(bot.id, { x = dx / length, y = dy / length, aim = math.atan2(dy, dx), fire = length < 600 })
    end
  end
end)
```

### Shared helpers with `require`

`lib/vector.lua`:

```lua
local vector = {}
function vector.length(x: number, y: number): number
  return math.sqrt(x * x + y * y)
end
return vector
```

`main.lua`:

```lua
local vector = require("lib.vector")
```

See `mods/gravity_wells` for a pack built this way.

### Bundled examples

| Pack | Shows |
| --- | --- |
| `mods/bounty_hunt` | a custom mode with timers, armor and shields |
| `mods/gravity_wells` | physics with `push`, spawning shapes, `require` |
| `mods/sprinkler` | a new tank attached to the upgrade tree from Luau |
| `mods/fast_arena` | a data pack that changes the rules |
| `mods/vampire` | a data pack with a JSON hook |
| `mods/workshop_toolkit` | workshop plugin tools and a map check |
| `mods/logic_kit` | workshop tools that build triggers, and a signal-loop check |

## Tools for pack authors

```sh
# Load and validate packs the way a server does, running their load phase and
# loading any workshop plugins.
tankyard-tools validate-mods --mod mods/bounty_hunt --mod path/to/pack.zip

# The safety review, from the command line. Exits 1 for review findings, 2 for danger.
tankyard-tools scan-mod mods/bounty_hunt path/to/pack.zip

# Time a pack in a busy arena.
tankyard-tools mod-bench --mod mods/gravity_wells --bots 40

# Weapon output and an autonomous tournament, for balance work.
tankyard-tools balance
tankyard-tools tournament --bots 24 --ticks 18000

# Draw every tank model to an SVG sheet.
tankyard-models --output artifacts/models/tanks.svg --classes triple_twin,battleship --columns 3
```

From a source checkout, run these with `cargo run --release --locked -p tankyard-tools -- …`. Damage totals help find outliers; they don't measure how strong a tank is in a real match.

## Trusted Rust server plugins

Server operators can also write native extensions in Rust. They implement `tankyard_core::modding::ServerPlugin`, receiving a read-only `World`, a `HookEvent` and validated `Commands`; register a factory with `UdpServer::register_plugin`, and the factory recreates the extension each round. [The example](../crates/tankyard-server/examples/plugin.rs) heals on kills:

```sh
cargo check --locked -p tankyard-server --example plugin
```

Native extensions run inside the server process with no sandbox: they can loop forever or touch the filesystem. Use them only for code you trust, and use script packs for everything you share.

## Troubleshooting

| Message | Meaning |
| --- | --- |
| `tankyard.X() is only available inside event handlers` | a query or command ran while loading, or from a function saved and called outside an event. Move it into a handler |
| `tankyard.on: unknown event "…"` | check the [event names](#events) |
| `invalid hook action or class reference` | a command was out of bounds, or named a class that isn't loaded. The whole batch was rejected |
| `per-tick command budget exceeded` | more than 512 commands in one tick. Spread the work across ticks |
| `instruction budget exceeded` | a handler ran too long. Do heavy work every few ticks, or keep state between ticks |
| `Luau has no goto` / `Luau has no goto labels` | rewrite the jump with `continue`, `break` or a flag; only `goto continue` to a label at the end of a loop body converts automatically |
| A pack shows **Needs review** | open **Safety review** to see each finding, its file and its line |

Server consoles show each pack's errors and `print` output. Offline, the Mods page shows a disabled pack's error.

Some content references for the built-in tanks are the Diep community pages for [Triple Twin](https://diepio-archive.fandom.com/wiki/Triple_Twin), [Battleship](https://diepio.fandom.com/wiki/Battleship), [Hunter](https://diepio.fandom.com/wiki/Hunter), [Skimmer](https://diepio.fandom.com/wiki/Skimmer), [Auto 3](https://diepio.fandom.com/wiki/Auto_3) and [Auto Smasher](https://diepio-archive.fandom.com/wiki/Auto_Smasher). Tank Yard draws its own geometry. The event hooks follow the general pattern of [Garry's Mod hooks](https://wiki.facepunch.com/gmod/Hook_Library_Usage), but Tank Yard doesn't load Garry's Mod addons.
