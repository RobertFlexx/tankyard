# Arena Workshop

Open **Map editor** on the main menu. The workshop builds maps for local games and community servers using the same validated map-pack format as scripted mods. The canvas fills the middle of the screen. The **dock** along the bottom changes with the mode, and the **inspector** on the right has six tabs: Map, Object, Logic, Check, Rules and Files.

## Build a first map

1. In **Build** mode (key **1**), pick **Blocks** in the dock and a style. Click to place one grid cell, or drag to draw a wall of any size.
2. Press **2** for **Edit**. Click an object to select it. Drag it to move it, and drag its handles to resize it. The Object tab shows its settings.
3. Add **Pads** for movement and combat zones, and **Decor** for floor art, labels and particles.
4. Name the arena in **Map**; the map id follows the name until you edit it yourself. Keep the two spawn regions clear.
5. Press **F5** to playtest an offline Sandbox match on your map. **Shift+F5** starts you at the pointer, and **Play here** at the middle of the view. **Escape returns to the editor**; the selection and view stay as they were.
6. **Save** writes an editable project. **Export** validates the map and writes a ZIP pack. Import that ZIP on **Mods**, enable it, and pick your arena in the map picker.

The workshop writes a recovery draft a few seconds after each change and restores it when you reopen it. The draft is a single latest copy, not a history. Save named projects before starting another map. **New** and **Back** ask for a second click when there are unsaved changes.

## Modes

**Build** places the chosen palette item:

- **Click** places one item.
- **Drag** draws one item at the dragged size. A line drawn along the grid still gets one cell of depth.
- **Swipe** (dock switch) paints a cell everywhere the pointer passes, skipping cells that already hold the same kind.
- **Right-click** any object to build copies of it, settings included. Pick a palette item to stop.

**Edit** selects and changes objects:

- **Click** selects an object; **Shift-click** adds or removes one.
- **Dragging empty floor** selects everything in the box.
- **Double-click** selects the object's whole group.
- **Drag** moves the selection, and the handles resize it.
- **Alt-drag** moves a copy and leaves the originals.

The dock holds nudges, the step size, turns, flips, smaller/larger, six alignments for two or more objects, front/back order, copy and paste, style copy, and group tools.

**Delete** removes what you click or swipe across. Dragging over empty floor removes everything in the box. The dock can also delete the selection, every object of the current palette kind, or the chosen group.

## Shortcuts

Press **F1** for the in-editor list. Ctrl means Cmd on a Mac.

| Keys | Action |
| --- | --- |
| 1, 2, 3 | Build, Edit, Delete |
| WASD, arrows | Nudge by the step (Shift: one unit); pan when nothing is selected |
| Q, E (or R) | Turn left, turn right |
| X, Y | Flip across, flip up and down |
| [ and ] | Send to back, bring to front |
| Delete, Backspace | Delete the selection |
| Esc | Cancel a drag, stop copying, or deselect |
| Ctrl+C, X, V | Copy, cut, paste at the pointer |
| Ctrl+D | Duplicate one cell over |
| Ctrl+Shift+C, V | Copy settings, then paste them onto matching objects |
| Ctrl+G | Put the selection in a new group |
| Ctrl+A | Select everything shown |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z) | Undo, redo (48 steps) |
| Ctrl+S | Save the project |
| Wheel, +, − | Zoom around the pointer |
| Middle-drag, Space-drag | Pan |
| F | Fit the arena |
| H, G, L | Show hidden groups, group labels, logic links |
| P, Shift+P | Run or pause the logic preview, restart it |
| Hold T | Test tank at the pointer for the preview |
| F5, Shift+F5 | Playtest, or start at the pointer |

Shortcuts pause while a text box has focus. Holding a nudge key repeats it, and the whole hold undoes as one step.

The **Grid** switch cycles through 10, 20, 40, 80, 160 and 320 world units; Shift-click steps back. With **Snap** off, objects follow the pointer exactly. The step button sets the nudge distance: one unit, half a cell, one cell or four cells.

## Inspector

Click any slider's value to type an exact number, then press Enter (Esc cancels). With several objects selected, a change applies to every object that has that setting. Repeated edits to one setting undo as one step.

- **Map:** name, id, description, size, recommended players, wall style, floor, and the groups hidden when a round starts. Objects keep their relative place when the size changes.
- **Object:** position, size, group and the settings for the selected kind. A group's *starts hidden* switch is available here, so a gate needs no trip to Map.
- **Logic:** preview controls, the test tank's team, the eight counters, a log of what happened, and every trigger with its summary. Click a trigger to find it.
- **Check:** runs the map checks. Red items stop playtests and exports; amber items are advice, such as a signal nobody listens to or a group with no objects. Click an item to select it.
- **Rules:** the movement, damage, health and score multipliers used by the playtest and the exported pack.
- **Files:** saved projects, pack import, the workshop folder, and built-in arenas to copy as templates.

## Objects

**Blocks** are solid cover. A block uses the arena's wall style or its own: slate, hedge, circuit, rampart, relay, orrery or sandstone. It can also take a color tint. The collider is always its rectangle.

**Pads** are zones that affect tanks whose centers are inside. Strength ranges from 0.25 to 2:

- **Speed:** movement × `(1 + 0.45 × strength)`.
- **Mud:** movement ÷ `(1 + 0.8 × strength)`.
- **Repair:** restores `6% × strength` of maximum health each second.
- **Hazard:** deals `8% × strength` of maximum health plus 6 health each second, subject to normal damage protection.
- **Shelter:** incoming damage ÷ `(1 + 1.5 × strength)`.
- **Conveyor:** carries tanks along its direction at `150 × strength` units a second, added to their own movement. Turning or flipping the pad turns its direction.
- **Portal:** sends a tank to the exit marker. Drag the diamond to move the exit. The exit must sit in open floor, and a tank must leave and wait 1.2 seconds before using a portal again.

Overlapping speed and mud pads multiply, and the final factor is clamped to 0.3–2.5. Overlapping conveyors add up to 600 units a second. Repair and hazard rates add; shelter pads multiply. Speed and conveyor effects run in shared code, so movement predicts smoothly online.

**Decor** includes tiles, stripes, lights, rings, arrows, labels and particle emitters. It has color, opacity, rotation, spin, pulse and an *above players* switch. It never changes collision. A label can show a counter: `{1}` to `{8}` are replaced with that counter's value.

**Markers** place spawn regions (neutral, team 1 or team 2), team bases and objectives. A team base needs a matching team spawn region around it.

## Groups and logic

Every block, pad, decor item and trigger can belong to a group from 1 to 32. Group 0 is always on. A hidden group's blocks stop colliding, its pads stop working, its decor disappears, and its triggers switch off. The dock's **Group** chooser puts newly built objects in a group; **Free** picks an unused one. Newly built Toggle, Show and Hide triggers act on that group instead of joining it. **G** shows group numbers on the canvas, and **L** draws arrows from each trigger to what it affects.

A trigger has three parts: **when** it fires, the **filters** that apply, and the **action** it takes.

When:

- **Every:** repeats every *Seconds*, starting after *Start offset*.
- **On enter / On exit:** a tank crosses into or out of the trigger's area. *Seconds* is the cooldown. Who and Team pick which tanks count: anyone, players only, bots only, and any team or teams 1–4. A tank that dies inside does not count as leaving.
- **At start:** once, *Seconds* after the round starts.
- **On signal:** another trigger sent a signal on its channel (1–32).
- **On counter:** its counter (1–8) reaches the threshold, from either direction.

Actions:

- **Toggle, Show, Hide:** change a group. *Restore after* puts the group back after that many seconds; 0 keeps the change.
- **Announce:** shows text to everyone, up to 96 characters. `{1}`–`{8}` insert counters.
- **Boss:** brings the chosen boss to the trigger. A crowded map or the 16-boss limit can prevent it.
- **Signal:** fires every *On signal* trigger on that channel.
- **Count:** adds an amount (negative subtracts) to a counter. Counters stay within ±99,999 and reset each round.
- **Shapes:** drops 1–24 shapes inside the trigger's area.

For every trigger:

- *Chance* is the percent chance it acts when it fires, and can make a coin flip.
- *Delay* waits before acting.
- *Once per round* stops it after its first action.

Signals and counters can chain. Each tick allows 256 chained actions, so a loop stops instead of freezing the server. Map changes and new rounds reset the logic.

### Logic preview

Press **P** to run the map's triggers in the editor with the same engine servers use. Timers count, groups switch on the canvas, and triggers flash when they act. Counters, announcements, bosses and shapes show up in the Logic tab. **Hold T** to drive a test tank at the pointer through plates and exits; the Logic tab sets its team and whether it counts as a bot. **Shift+P** restarts. Editing a trigger restarts the preview by itself.

### Recipes

A pressure gate:

1. Choose a free group in the dock (**Free**), then draw a wall. Switch on *Group N starts hidden* in Object.
2. From **Logic**, place a **Plate** beside it. It already shows that group for 5 seconds when a tank enters.
3. Press **L** to check the arrow, then hold **T** over the plate with the preview running.

A three-switch door:

1. Place three **Counter** triggers from Logic. Each adds 1 to counter 1 when a tank enters; switch on *Once per round* for each.
2. Place a **Goal**, which watches counter 1. Set its threshold to 3 and its action to Hide, targeting the door's group.
3. Add a label reading `Switches {1}/3` above the door.

Avoid closing solid cover on spawns or objectives. Map checks cover the starting layout and connectivity; they can't prove that every combination of groups stays passable.

## Particle generator

Place an emitter from **Decor**. Its Object tab has presets (Embers, Rain, Dust, Sparks) and every setting: shape, rate, lifetime, speed, direction, spread, gravity, drag, start and end size, spin, fade and both colors. Direction uses screen coordinates: 0° right, 90° down. Particles are born across the emitter's area.

Particles are drawn from seeds, so they cost the server nothing and send no network traffic. A map has at most 48 emitters, each with at most 240 live particles, and the client draws at most 1,024 map particles, scaled by the Effects setting. Reduced motion hides map particles and freezes spin, pulse and conveyor marks.

## Rules

The Rules tab sets movement, acceleration, coasting, damage, health, regeneration, experience, score and respawn time for the playtest and the exported pack. Standard, Arcade, Endurance and Drift are starting profiles. These are map-pack rules, not permissions or server security settings.

An authorized host can change the same rules during a match with **Server → Rules**. Health changes keep each tank's health percentage, and players keep their upgrades and scores. Customized matches do not earn normal arena records.

## Files and deployment

The workshop folder is `workshop/` inside Tank Yard's data directory. **Files → Open folder** reveals it. It contains:

- `draft.json`: the recovery draft.
- `projects/<map-id>.json`: named editable documents.
- `exports/<map-id>-<timestamp>.zip`: portable exports.

Each export contains `mod.json` and `editor.json`. Gameplay uses the manifest, and the editor document lets others keep editing. The ZIP holds no compiled code. Maps that use conveyors, portals, signals, counters, filters or shapes need protocol 25 (Tank Yard 0.15) on both server and clients.

For a dedicated server, put the ZIP in persistent storage, add its path to the top-level `mods` list, and run `reload-mods`. Then `maps` lists it and `map my_arena` selects it. Joining clients receive the checked map data automatically. See [Hosting](hosting.md) and [domain setup](hosting-domain.md).

Export and playtest check dimensions, ids, text, bounds, object budgets, trigger settings, portal exits, spawn and base clearance, objectives and starting connectivity. A failed export leaves the document editable and shows the reason in Check. The schema is documented in [Modding](modding.md#maps).
