# Arena Workshop

Open **Map editor** on the main menu. The workshop builds maps for local games and community servers using the same validated map-pack format as scripted mods. Its build/select/erase workflow takes inspiration from level editors such as Geometry Dash, with Tank Yard's own art, tools and arena mechanics.

## Build a first map

1. Choose **Blocks** in the left palette, then a block style. Drag on the canvas to draw solid cover. A click stamps one grid cell.
2. Press **1** for Select. Click an object to show its properties. Drag it to move it; drag the square at its bottom-right corner to resize it.
3. Add **Pads** for movement or combat zones. Add floor panels, signs or particles from **Decoration**.
4. Set a name and a lowercase map ID in **Map**. Keep the spawn regions clear; the starting map has two neutral spawn regions in opposite corners.
5. Press **Playtest** or **F5**. This starts an offline Sandbox match on your current map, with two bots, your pack's rules and a fully leveled player. **Escape returns to the editor** with the document intact.
6. **Save** creates an editable project. **Export pack** validates the map and writes a portable ZIP. Import that ZIP on **Mods**, enable it, then select your arena in the map picker.

The workshop keeps a recovery draft every few seconds after edits. It restores that draft when reopened. Save named projects before starting another map; the recovery draft is one latest document, not a version history. Back and New ask for a second click when there are unsaved changes.

## Controls

- **1 / 2 / 3:** Select / Build / Erase. Right-click returns to Select and clears the selection.
- **Wheel:** zoom around the pointer on the canvas; scroll properties while over the right panel.
- **Middle drag** or **Space + left drag:** pan. **F** fits the whole arena.
- **Shift-click:** add or remove an object from the selection. Drag empty space to box-select.
- **Arrow keys:** nudge selected objects by a grid cell. **Shift + arrows:** nudge by one world unit.
- **Ctrl/Cmd+C, X, V, D:** copy, cut, paste, duplicate. Copies preserve properties and groups; pasted objects move one grid cell.
- **Delete / Backspace:** remove the selection. **Ctrl/Cmd+Z:** undo; **Ctrl/Cmd+Shift+Z** or **Ctrl+Y:** redo. History holds 32 edit operations.
- **R:** rotate. Collision rectangles rotate 90 degrees, preserving world dimensions; decorations rotate in 15-degree steps. **Flip X / Flip Y** mirror the selection's positions.
- **Ctrl/Cmd+S:** save. Keyboard shortcuts pause while a text field has focus.

The grid cycles through 10, 20, 40, 80 and 160 world units. Turn Snap off for free placement. Terrain collision remains axis-aligned even when decorative art is rotated.

## Objects

**Blocks** are solid cover. Styles include slate, hedge, circuit, rampart, relay, orrery and sandstone. A block can use the arena's default style or its own, plus an optional color tint. The collider is always its rectangle; detail stays within the cover silhouette.

**Pads** are rectangular zones affecting tanks whose centers are inside them. Strength ranges from 0.25 to 2:

- **Speed:** movement × `(1 + 0.45 × strength)`.
- **Mud:** movement ÷ `(1 + 0.8 × strength)`.
- **Repair:** restores `6% × strength` of maximum health each second.
- **Hazard:** deals `8% × strength` of maximum health plus 6 health each second, subject to normal damage protection.
- **Shelter:** incoming damage ÷ `(1 + 1.5 × strength)`.

Overlapping speed and mud pads multiply, with the final terrain movement factor clamped to 0.3–2.5. Repair/hazard rates add; overlapping shelter pads multiply. Speed changes are calculated with shared server/client code during prediction.

**Decoration** includes tiles, warning stripes, lights, rings, arrows, painted text and particle emitters. Choose color, opacity, rotation, spin, pulse frequency and whether it draws above players. Decorative rotation and particles do not change collision. Above-player effects should remain sparse enough to keep combat readable.

**Spawns** include spawn regions, one base position per team (1–4), and objective positions. A team base needs a matching team spawn region around it. Neutral spawns use team 0. The simulator reserves team territory and still verifies safe spawn clearance.

**Files** opens saved projects, imports an exported map pack and copies built-in maps as templates. Procedural maze/garden cover is baked into editable walls when copying a template. Importing a pack without `editor.json` edits its first map; other scripts/content in that pack are not copied into the new map-only export.

## Groups and triggers

Objects can belong to group 1–32; group 0 has no visibility switch. The build-group selector assigns new blocks, pads and decoration to that group. Use Object to change one item's group, or select several and assign them together. In Map, select the groups hidden when the round starts. **Show hidden** lets you edit those objects without changing their starting state.

Triggers operate on groups, post an announcement or spawn a boss. They run on the authoritative simulation; only the resulting group mask and world state reach players.

- **Every:** repeats after the period in Seconds. Start offset delays the schedule.
- **Enter:** fires when a living tank crosses into the trigger rectangle. Seconds is a cooldown. Standing inside does not repeat it; a later crossing can fire again.
- **Start:** fires once, Seconds after the round/map begins.
- **Toggle / Show / Hide:** change the target group's visibility and collision.
- **Announce:** sets the round's announcement text.
- **Boss:** brings the chosen boss to the nearest safe point near the trigger. A crowded map or the 16-boss limit can prevent the spawn.

Restore after temporarily changes a group and then restores its previous state. Zero keeps the change. A later action on the same group cancels its pending restoration. Once per round prevents a repeating or entry trigger from firing again. Map changes and new rounds reset the schedule.

A timed gate recipe:

1. Build a gate block and assign group 3.
2. Add a Toggle trigger targeting group 3.
3. Set Every, Seconds 10, Restore after 4.
4. Playtest: the gate hides after ten seconds, then returns four seconds later.

Avoid closing solid cover on spawns or objectives. Map validation checks starting geometry and connectivity; it cannot prove that every possible scripted group combination stays navigable.

## Particle generator

Place a **Particles** object from Decoration. Select it and open **FX**. Animate FX previews it on the canvas; playtesting shows the same effect in the arena.

Start with Embers, Rain, Dust or Sparks, then edit shape, rate, lifetime, speed, direction, spread, gravity, drag, initial/final size, spin, fade and both colors. Direction uses screen coordinates: 0° right, 90° down, 180° left, 270° up. The emitter's rectangle distributes particle births across its area.

The renderer reconstructs particles from their birth time and deterministic seeds. Emitters do no server simulation work and send no per-particle network packets. A map has at most 48 emitters, each at most 240 live particles; the client draws at most 1,024 map particles total, scaled by its Effects setting and culled off-screen. Reduced motion suppresses map particles and freezes decorative spin/pulse.

## Rules

The Rules tab sets movement, acceleration, coasting, damage, health, regeneration, experience, score and respawn time for the playtest/exported pack. Standard, Arcade, Endurance and Drift provide starting profiles. These are map-pack rules, not permissions or server security settings.

An authorized host can adjust the same rules during a match using **Server → Rules**. Health changes keep each tank's current health percentage; players keep their identity, upgrades and scores. Existing projectiles retain their original damage; subsequent shots use the updated multiplier. Customized matches do not earn normal arena records.

## Files and deployment

The workshop folder is `workshop/` inside Tank Yard's data directory. **Files → Open workshop folder** reveals it. It contains:

- `draft.json`: the recovery draft.
- `projects/<map-id>.json`: named editable documents.
- `exports/<map-id>-<timestamp>.zip`: separately named portable exports.

Each export contains `mod.json` and `editor.json`. Gameplay uses the manifest; the editor document allows source editing. The ZIP contains no compiled client and no native code. It can be shared with players or kept private by a server owner. The current extended map schema needs a build supporting protocol 24.

For a dedicated server, place the ZIP in persistent storage, add its path to the top-level `mods` list, and run `reload-mods`. Then `maps` lists it and `map my_arena` selects it. Joining clients receive checked map data automatically. See [Hosting](hosting.md) and [domain setup](hosting-domain.md).

Export/playtest checks dimensions, IDs, text, bounds, object budgets, spawn/base clearance, objectives and starting connectivity. A failed export leaves the document editable and displays the reason. Drafts can be incomplete; source code and schema are documented in [Modding](modding.md#maps).
