# Editor plugins

Arena Workshop plugins are Luau packs that add map tools and checks to the editor. The workshop builds each tool's controls from its parameter definitions. A tool receives a copy of the current map and can place, remove or change objects. Accepted changes form one undo step.

Plugins require Tank Yard 0.15.1 or later and use `api_version` 2. Save Luau source as UTF-8 files with the `.lua` extension. [Workshop tutorial](arena-workshop-tutorial.mp4) · [Editor controls](map-editor.md) · [Map schema](modding.md#maps)

## Install and use a plugin

1. Open **Mods**, choose **Browse for a pack…**, and select a pack's `mod.json` or ZIP.
2. Read any review findings and enable the pack. Approval for flagged packs covers their exact contents.
3. Open **Map editor → Tools**. If the editor was already open, press **Reload plugins**.
4. Click a location on the canvas, set the tool's controls, then press its **Run** button.
5. Press **Ctrl+Z** to undo the result or **Ctrl+R** to run the last tool again. Ctrl means Command on macOS.

The Tools tab shows load errors, packs waiting for review, and **Last run** messages. **Show/Hide** folds a pack's tools, and the editor remembers that choice. Plugin checks appear as amber notes when **Check** runs. Clicking a note can select the object it describes.

Workshop Toolkit supplies Repeat, Ring of blocks, Arena frame, Snap to grid and Four-way mirror. Logic Kit supplies Switch door, Countdown and Sequence; its 0.16 Lever bank tool builds a circuit with several levers, watchers and an AND, OR or XOR gate. Both packs are included with the game and can be enabled on Mods. The [logic gates tutorial](logic-gates-tutorial.md) explains the circuit. [Workshop Toolkit source](../mods/workshop_toolkit/tools.lua) · [Logic Kit source](../mods/logic_kit/tools.lua)

## Create a first tool

Create a folder containing `mod.json` and `tools.lua`. Put both files at the root when distributing the folder as a ZIP.

Use this `mod.json`.

```json
{
  "api_version": 2,
  "id": "block_row",
  "name": "Block Row",
  "version": "1.0.0",
  "author": "Your name",
  "description": "Place a row of blocks at the canvas pointer.",
  "editor": ["tools.lua"]
}
```

Use this `tools.lua`.

```lua
editor.tool {
  id = "row",
  name = "Row of blocks",
  description = "Place a row, keeping it inside the arena.",
  params = {
    { id = "count", name = "Blocks", type = "integer", min = 1, max = 20, default = 4 },
    { id = "size", name = "Block size", type = "number", min = 10, max = 160, step = 10, default = 40 },
    { id = "gap", name = "Gap", type = "number", min = 0, max = 160, step = 10, default = 40 },
    { id = "style", name = "Style", type = "choice", options = { "slate", "garden", "circuit" }, default = "slate" },
  },
  run = function(map, params, ctx)
    local span = params.count * params.size + (params.count - 1) * params.gap
    if span > map.width or params.size > map.height then
      error("Use fewer or smaller blocks to fit this arena.")
    end
    if #map.walls + #map.blocks + params.count > 2048 then
      error("This row would exceed the map's solid-object budget.")
    end

    local pointer = ctx.pointer or { x = map.width / 2, y = map.height / 2 }
    local x = math.clamp(pointer.x, 0, map.width - span)
    local y = math.clamp(pointer.y, 0, map.height - params.size)
    for i = 1, params.count do
      table.insert(map.blocks, {
        rect = editor.rect(map, x + (i - 1) * (params.size + params.gap), y, params.size, params.size),
        style = params.style,
        group = 0,
      })
      ctx.select("blocks", #map.blocks)
    end
    ctx.say(string.format("Placed %d blocks.", params.count))
  end,
}

editor.check {
  id = "small_blocks",
  name = "Small blocks",
  description = "Find blocks that may be hard to select or see.",
  run = function(map)
    local found = {}
    for i, block in ipairs(map.blocks) do
      local _, _, width, height = editor.world(map, block.rect)
      if width < 20 or height < 20 then
        table.insert(found, {
          text = "This block is thinner than 20 world units.",
          list = "blocks",
          index = i,
        })
      end
      if #found == 64 then break end
    end
    return found
  end,
}
```

Validate the manifest, source and tool registrations from the repository root.

```sh
cargo run --locked -p tankyard-tools -- validate-mods --mod path/to/block_row
cargo run --locked -p tankyard-tools -- scan-mod path/to/block_row
```

For development, load the folder directly so Reload plugins picks up saved edits.

```sh
cargo run --locked --release -p tankyard-client --bin tankyard -- --page editor --mod path/to/block_row
```

Run **Row of blocks**, change its parameters and run it again, then try undo and redo. Run **Check** to see the advice from Small blocks. A playtest or export also checks the map's complete geometry and connectivity.

## Pack layout and loading

`mod.json` uses the normal pack manifest. Its `editor` list names the entry scripts, executed in list order. A pack can list up to 16 editor entry scripts. All entries in one pack share a Luau state, tool registrations and helper modules.

A pack containing only editor scripts loads in the workshop and has no effect on a match's modded status. A pack can also declare gameplay `scripts`, maps or other content. The `editor` entries execute in the workshop, and gameplay `scripts` execute during simulation. Exported maps carry the objects and triggers the tools created. Players joining a server receive validated map data, and map authors install the plugins they use to edit it.

Keep pack IDs stable between releases. Mods remembers enabled packs by ID, and an imported pack takes precedence over a bundled pack with the same ID. Increment the manifest's `version` when distributing an update. Optional descriptions, icons and source metadata belong in `tankyard.toml`. [Pack manifest and metadata](modding.md#kinds-of-packs)

## Register a tool

Call `editor.tool(spec)` while an entry script or helper module loads. The specification has these fields.

- `id` is a unique tool ID within the pack, from 1 to 48 UTF-8 bytes. Stable lowercase IDs make updates easier to track.
- `name` is the displayed title, from 1 to 48 UTF-8 bytes.
- `description` is optional text, from 1 to 200 UTF-8 bytes when supplied.
- `params` is an optional list of up to 12 controls. Parameter IDs must be unique within the tool.
- `run` is a required function with arguments `map`, `params` and `ctx`.

A pack can register 32 tools. `params` passes control values under their IDs, such as `params.count`. Mutate the supplied map and return nothing, or return a replacement map table. Use `error("message")` for an actionable failure message.

The workshop deserializes the result, checks its object budgets, dimensions, coordinates, rules and visual features, then applies the accepted document as one undo step. Triggers may be unfinished while editing. **Check**, playtest and export validate their behavior and the complete navigable layout.

## Parameter types

Every parameter needs an `id` of 1 to 32 UTF-8 bytes. `name` defaults to the ID and allows up to 40 characters. The default `type` is `number`.

### Number

```lua
{ id = "spacing", name = "Spacing", type = "number", min = 0, max = 400, step = 10, default = 80 }
```

`min` defaults to 0 and `max` to 100, with `min < max`. `default` defaults to `min` and is clamped into the interval. `step` defaults to the larger of `(max - min) / 100` and `0.01`, and must be positive. These numeric fields must be finite and within ±10,000,000. The callback receives a number.

### Integer

```lua
{ id = "copies", name = "Copies", type = "integer", min = 1, max = 20, default = 4 }
```

Use whole-number bounds and defaults. Bounds have the same numeric limits as Number. The callback receives a whole number. The controls round integer entries and use steps of one.

### Toggle

```lua
{ id = "hidden", name = "Start hidden", type = "toggle", default = false }
```

The default is `false`. The callback receives `true` or `false`.

### Choice

```lua
{ id = "style", name = "Style", type = "choice", options = { "slate", "garden" }, default = "garden" }
```

Supply 1 to 24 nonempty option names, each up to 32 characters. `default` names an option and falls back to the first option if omitted or unmatched. The callback receives the selected name.

### Text

```lua
{ id = "label", name = "Label", type = "text", limit = 64, default = "Start" }
```

`limit` defaults to 64 and is bounded to 1–200 characters. The default text is empty, and longer defaults are shortened to the limit. The callback receives a string.

## Context and selection

The `ctx` argument describes the editor when the tool runs.

- `ctx.selection` is a list of `{ list = "blocks", index = 3 }` references. Indexes start at **1**.
- `ctx.pointer` is `{ x, y }` in world units. Ctrl+R over the canvas uses the current pointer. A Run button uses the last canvas click, falling back to the middle of the view.
- `ctx.grid` is the grid size in world units.
- `ctx.view`, when available, is `{ x, y, w, h }` in world units for the visible part of the arena.
- `ctx.select(list, index)` selects a resulting object. Call it repeatedly to select several objects, up to 4,096 references. Calling it replaces the previous selection when the tool finishes. With no calls, the existing selection is kept and invalid references are discarded.
- `ctx.say(text)` adds a result message. Up to eight messages are retained, each displayed as one line of up to 160 characters. If several arguments are passed, the first supplies the message.

The supported list names are `walls`, `blocks`, `zones`, `decor`, `triggers`, `spawn_regions`, `team_bases` and `objective_positions`.

Resolve a selected object with `map[ref.list][ref.index]`. Removing an item shifts later indexes in that list. Iterate backwards when removing several objects, and choose output references after the edits are complete.

```lua
for _, ref in ipairs(ctx.selection) do
  if ref.list == "blocks" then
    map.blocks[ref.index].style = "garden"
  end
end
```

## Map data and coordinates

`map` follows the public map schema. Its width and height are in world units. Rectangles and positions are fractions of the arena, with positive Y pointing down.

- `walls` contains `{ x, y, width, height }` arrays.
- `blocks`, `zones`, `decor`, `triggers` and `spawn_regions` contain objects with a `rect` array.
- `team_bases` contains `{ team, position = { x, y } }` objects.
- `objective_positions` contains `{ x, y }` arrays.
- `hidden_groups` lists the groups hidden when a round begins.

Use the helpers to convert coordinates.

```lua
local rect = editor.rect(map, 200, 300, 80, 40)
local point = editor.point(map, 600, 400)
local x, y, width, height = editor.world(map, rect)
local px, py = editor.world(map, point)
```

`editor.rect` returns four normalized numbers. `editor.point` returns two. `editor.world` returns two values for a position or four for a rectangle. Keep solid geometry and spawn regions within the arena and preserve the map's spawn and navigation requirements. Map-wide rules are edited on the workshop's **Rules** tab. [Complete map fields, styles, pads, effects and trigger schema](modding.md#maps)

## Allocate groups, signals and counters

Use `editor.free_group(map)`, `editor.free_channel(map)` and `editor.free_counter(map)` to find the lowest unused number. They return `nil` when exhausted.

- Groups range from 1 to 32. The helper checks block, zone, decoration and trigger memberships, visibility, motion and Reset triggers actions, On shown, On hidden and On arrive watchers, group conditions and starting hidden groups.
- Signal channels range from 1 to 32. The helper checks signal listeners, signal actions, otherwise channels and the ranges used by Random signal and Sequence actions.
- Counters range from 1 to 8. The helper checks On counter and On change listeners, Count, Set and Calculate actions, counter operands and every condition's counters.

`editor.conditions(trigger)` returns a trigger's conditions as one list, `only_if` first and then `also_if`, or an empty list when it has no gate. Use it to read gates without handling the two fields apart. When you build a gate, set `only_if` to the first condition and `also_if` to the rest, at most three, and `gate` to how they combine; the [trigger schema](modding.md#maps) lists every field. Gates of several conditions, otherwise signals and the newer When kinds and actions need Tank Yard 0.16.0; the bundled Logic Kit's Lever bank tool shows them in use.

Each helper reads the map as it exists at that call. Reserve an allocated number by adding its object or trigger before asking for another number of the same kind.

```lua
local group = editor.free_group(map)
if not group then
  error("All map groups are in use.")
end
for _, ref in ipairs(ctx.selection) do
  if ref.list == "blocks" then
    map.blocks[ref.index].group = group
  end
end
```

The Logic Kit source contains complete examples of tools that connect groups, signals, counters and triggers. Runtime Script event actions communicate with gameplay Luau through `map_event` and the map commands. [Logic Kit](../mods/logic_kit/tools.lua) · [Gameplay map scripting](modding.md#map-logic)

## Register a map check

Call `editor.check(spec)` with `id`, `name`, an optional `description`, and `run(map)`. The ID, name and description limits match tool registration. Check IDs are unique within the pack's checks, and a pack can register 32 checks.

Return a list of strings or finding tables. A finding table needs `text` and can include `list` and a one-based `index` to select an object when clicked.

```lua
return {
  "Consider adding cover near this spawn.",
  { text = "This entrance is narrow.", list = "blocks", index = 1 },
}
```

Checks receive a map copy, and their return value supplies advice in the **Check** tab. Findings are prefixed with the plugin name. The first 64 findings per pack are retained, with each message limited to 200 characters. The workshop's built-in checks determine whether a playtest or export can proceed.

## Modules, state and execution limits

Use `require("lib.layout")` for `lib/layout.lua` or `lib/layout/init.lua` inside the same pack. Module results are cached within that pack's Luau state. Global and module state persists across tool runs until **Reload plugins** or the editor's state is recreated. Store settings that need to survive distribution or a fresh editor run in the map data or parameter defaults.

The sandbox provides computation libraries, including `math`, `string`, `table`, `bit`, `bit32`, `utf8`, `buffer` and `coroutine`. Map editing uses `editor`, the supplied map and `ctx`. Gameplay callbacks use the separate `tankyard` API in gameplay entry scripts. Filesystem, process, network and native-library access are excluded from the plugin environment. [Sandbox reference](modding.md#sandbox-and-limits)

- Loading a pack's editor scripts has a 20,000,000-step budget.
- Each tool run has a 20,000,000-step budget.
- All checks in one pack share a 20,000,000-step budget for a Check operation.
- Each pack's Luau state has a 64 MiB memory budget.
- `print` output is bounded to 64 lines. **Last run** shows up to eight result and log lines.
- Ordinary pack limits allow 256 files, 2 MiB per file and 8 MiB in total, with a 1 MiB manifest.

A plugin that exceeds its execution budget stops until **Reload plugins**. A runtime error is shown under Last run, and an invalid tool result is rejected before changing the current document. Plugin state can still retain work done before a callback failed, so initialize each run from its arguments and reserve persistent state for deliberate uses.

## Develop and distribute

1. Keep a development folder and run it with `--mod` while editing source.
2. Run `validate-mods` and `scan-mod` after changing the manifest or registrations. The validator loads scripts and registrations. Exercise callback behavior by running the tools and checks in the workshop.
3. Check empty selections, large maps, arena edges, exhausted groups and counters, undo/redo, and repeated runs with changed parameters.
4. Press **Reload plugins** after saving source changes. If a pack was imported as a ZIP, import the updated ZIP to replace its installed copy.
5. Zip the pack with `mod.json` at the root, then test importing it on Mods and running its tools.

Use stable pack, tool and parameter IDs. Parameter values are retained during an editor instance, so reloads should preserve compatible parameter types and order. Add a README describing what each tool changes and the minimum supported game version.

## Troubleshooting

- **The pack is absent from Tools.** Check that it is enabled on Mods, has `api_version` 2 and lists its `.lua` entry files under `editor`. Review any held-pack messages, then reload.
- **The pack adds no tools or checks.** An entry script must call `editor.tool` or `editor.check` while loading. A helper file runs when an entry requires it.
- **A tool fails to load.** Check duplicate IDs, required functions, parameter bounds and missing entry files with `validate-mods`.
- **A tool stops.** Read the reported budget failure. Bound loops and findings, reduce pairwise comparisons on large maps, then reload after changing the source.
- **The map result is rejected.** Check normalized coordinates, finite numbers, positive object sizes, supported field names and object budgets. The Tools message reports the failed draft check.
- **A generated map fails playtest or export.** Run Check, select the reported objects and inspect trigger targets, spawn clearance and navigable connectivity.
- **The tool places objects at the wrong scale.** Use world units for `ctx.pointer`, `ctx.grid` and `ctx.view`, and convert through `editor.rect` or `editor.point` before writing map geometry.
- **The selection points at different objects after deletion.** Remove list items backwards and call `ctx.select` using indexes in the resulting map.
- **Saved source edits have no effect.** Check whether the active pack is an imported copy. Reimport an updated archive or use a development folder and Reload plugins.
