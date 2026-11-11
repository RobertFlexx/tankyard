# Arena Workshop

[Watch the tutorial](https://github.com/RobertFlexx/tankyard/releases/download/v0.15.1/arena-workshop-tutorial.mp4) for building, editing, map logic, plugin tools and playtesting in the workshop.

Open **Map editor** on the main menu. The workshop builds maps for local games and community servers using the same validated map-pack format as scripted mods. The canvas fills the middle of the screen. The **dock** along the bottom changes with the mode, and the **inspector** on the right has seven tabs: Map, Object, Logic, Check, Rules, Files and Tools.

Drag the edge between the canvas and the inspector or the dock to resize that panel; double-click the edge to put it back. Press **\\** to hide both panels and work on the canvas alone, and again to bring them back. Panel sizes, the grid, snap, the nudge step and the canvas switches are remembered between sessions.

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
- **Drag** moves the selection, and the handles resize it. With several objects selected, the handles stretch the whole selection: positions and sizes scale together, and markers keep their size.
- Hold **Shift** on a handle to keep the shape, or **Alt** to resize from the center.
- **Alt-drag** moves a copy and leaves the originals.
- **Ctrl+L** locks the selection's groups, so clicks and box selections pass through them to what lies underneath. The Object tab has the same switch, and **Ctrl+Shift+L** unlocks every group. Locks only affect the editor; group 0 cannot be locked.

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
| Ctrl+D | Duplicate one cell over; after you move the copy, Ctrl+D repeats that move |
| M, Shift+M | Mirror a copy of the selection across the arena, or up and down |
| Ctrl+Shift+C, V | Copy settings, then paste them onto matching objects |
| Ctrl+G | Put the selection in a new group |
| Ctrl+L, Ctrl+Shift+L | Lock the selection's groups, unlock every group |
| Ctrl+A | Select everything shown |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z) | Undo, redo (48 steps) |
| Ctrl+S | Save the project |
| Ctrl+R | Run the last plugin tool again |
| Wheel, +, − | Zoom around the pointer |
| Middle-drag, Space-drag | Pan |
| F, Shift+F | Fit the arena, frame the selection |
| \\ | Hide or show the panels |
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
- **Logic:** preview controls, the test tank's team, the eight counters, a log of what happened, and every trigger with its summary. Click a trigger to find it, or press its **Fire** button to make it act once right now, as if its moment had come.
- **Check:** runs the map checks. Red items stop playtests and exports; amber items are advice, such as a signal nobody listens to, a group with no objects, or a note from a plugin. Click an item to select it. **Room left** shows how much of each object budget the map uses.
- **Rules:** the movement, damage, health and score multipliers used by the playtest and the exported pack.
- **Files:** saved projects, pack import, the workshop folder, and built-in arenas to copy as templates.
- **Tools:** tools from [workshop plugins](#workshop-plugins), each with its own settings and a Run button.

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

A trigger has four parts: **when** it fires, the **filters** that apply, an optional **gate**, and the **action** it takes.

When:

- **Every:** repeats every *Seconds*, starting after *Start offset*.
- **On enter / On exit:** a tank crosses into or out of the trigger's area. *Seconds* is the cooldown. Who and Team pick which tanks count: anyone, players only, bots only, and any team or teams 1–4. A tank that dies inside does not count as leaving.
- **While inside:** acts as soon as *Tanks needed* noticed tanks are in the area, then again every *Seconds* while they stay. Good for capture points.
- **On empty:** the last noticed tank leaves the area or is destroyed.
- **On destroy:** a noticed tank is destroyed inside the area.
- **At start:** once, *Seconds* after the round starts.
- **On signal:** another trigger sent a signal on its channel (1–32).
- **On counter:** its counter (1–8) reaches the threshold, from either direction.

Actions:

- **Toggle, Show, Hide:** change a group. *Restore after* puts the group back after that many seconds; 0 keeps the change.
- **Announce:** shows text to everyone, up to 96 characters. `{1}`–`{8}` insert counters.
- **Boss:** brings the chosen boss to the trigger. A crowded map or the 16-boss limit can prevent it.
- **Signal:** fires every *On signal* trigger on that channel.
- **Random signal:** sends on one channel picked at random from a run of 2 or more, starting at *First channel*.
- **Count:** adds an amount (negative subtracts) to a counter. Counters stay within ±99,999 and reset each round.
- **Set counter:** puts a counter at an exact value. Goals watching that value act as if it had been counted to.
- **Shapes:** drops 1–24 shapes inside the trigger's area.
- **Repair tanks, Damage tanks:** restore or take 1–100% of maximum health from every noticed tank inside the area. Damage goes through normal damage protection.
- **Award points:** gives 1–100,000 points to every noticed tank inside the area.
- **Calculate:** changes a counter with a number or with another counter: add, subtract, multiply, divide (rounding down), remainder, lowest, highest, copy, a random number below the other value, or flip between 0 and 1. Dividing by zero changes nothing.
- **Script event:** raises a named event for script packs (see [Modding](modding.md#map-logic)). Scripts can answer by sending signals, changing counters and showing or hiding groups. Without a script pack it does nothing, and the preview only logs it.

The tank actions act on tanks inside the trigger's own area. On a timer, signal or counter trigger, *Who* and *Team* appear under the action to choose those tanks.

**Only if** adds a gate: when the trigger fires, it acts only if a counter compares as chosen (=, ≠, <, >, ≤ or ≥ against a number or another counter), or a group is shown or hidden. A failed gate does nothing and doesn't use up *Once per round* or the rest time. Gates make logic gates out of plain triggers: an AND is two gates in a row, an OR is two senders on one channel, and a NOT is a gate that checks for 0. Gated triggers show a small diamond on their badge.

For every trigger:

- *Chance* is the percent chance it acts when it fires, and can make a coin flip.
- *Delay* waits before acting.
- *Once per round* stops it after its first action.

Signals and counters can chain. Each tick allows 256 chained actions, so a loop stops instead of freezing the server. Map changes and new rounds reset the logic.

### Logic preview

Press **P** to run the map's triggers in the editor with the same engine servers use. Timers count, groups switch on the canvas, and triggers flash when they act. Counters, announcements, bosses, shapes, repairs, damage and points show up in the Logic tab. **Fire** on a trigger's row starts the preview if needed and makes that trigger act once, along with anything its signals and counters set off. **Hold T** to drive a test tank at the pointer through plates and exits; the Logic tab sets its team and whether it counts as a bot. **Shift+P** restarts. Editing a trigger restarts the preview by itself.

### Recipes

A pressure gate:

1. Choose a free group in the dock (**Free**), then draw a wall. Switch on *Group N starts hidden* in Object.
2. From **Logic**, place a **Plate** beside it. It already shows that group for 5 seconds when a tank enters.
3. Press **L** to check the arrow, then hold **T** over the plate with the preview running.

The Logic palette also has ready-made triggers: **Capture** (counts while a tank holds the area), **Bonus** (points for the first tank there), **Medic** and **Trap** (repair or damage on entry), **Dice** (a random signal every 15 seconds), **Reset** (puts a counter back to 0 when the area empties), **Gate** (passes signal 1 on as signal 2 while counter 1 is at least 1), **Lever** (flips counter 1 between 0 and 1) and **Script** (raises the script event `door`).

A three-switch door:

1. Place three **Counter** triggers from Logic. Each adds 1 to counter 1 when a tank enters; switch on *Once per round* for each.
2. Place a **Goal**, which watches counter 1. Set its threshold to 3 and its action to Hide, targeting the door's group.
3. Add a label reading `Switches {1}/3` above the door.

A capture point:

1. Place a **Capture** trigger over the point. While a tank is inside, it adds 1 to counter 1 every second.
2. Place a **Goal** watching counter 1 with a threshold of 10 and an Announce action, such as `Point taken!`.
3. Add a **Reset** over the same area so leaving it starts the count again, and a label reading `Hold {1}/10`.

A lever-operated gate:

1. Place a **Lever** on one side of the arena. Each time a tank enters, counter 1 flips between 0 and 1.
2. Place a **Goal** watching counter 1 with a threshold of 1 and a Hide action on the gate's group, so the gate opens when the lever turns on.
3. Duplicate the Goal, set its threshold to 0 and its action to Show, so the gate closes when the lever turns off.
4. Add a label reading `Lever {1}` so players can see its state.

Avoid closing solid cover on spawns or objectives. Map checks cover the starting layout and connectivity; they can't prove that every combination of groups stays passable.

## Workshop plugins

Packs can add tools and checks to the workshop. Enable a plugin pack on **Mods** (a pack from outside the bundled set needs approval first), then open the **Tools** tab. Each tool shows its settings and a Run button. A run changes the map as one undo step and usually selects what it made; **Ctrl+R** runs the last tool again with the same settings. Plugin checks add amber notes to the Check tab and never block a playtest or export.

The bundled **Workshop Toolkit** adds Repeat, Ring of blocks, Arena frame, Snap to grid and Four-way mirror, and a check for gaps too narrow to drive through. The bundled **Logic Kit** builds logic for you: **Switch door** turns the selected blocks into a door with switches that open it when all or any are pressed, **Countdown** counts a free counter down from the start of the round and announces a message at zero, and **Sequence** makes the selected triggers act one after another when a tank enters a new plate. It also notes signal loops that would run until the arena cuts them short. Plugin packs only change the editor, so enabling one never marks a match as modded. After editing a plugin's scripts, press **Reload plugins**. A tool's result passes the same checks as an import before the workshop keeps it, and a tool that errors or runs too long changes nothing. Writing plugins is covered in [Modding](modding.md#workshop-plugins).

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
- `prefs.json`: panel sizes and canvas switches.

Each export contains `mod.json` and `editor.json`. Gameplay uses the manifest, and the editor document lets others keep editing. The ZIP holds no compiled code. Maps that use conveyors, portals, signals, counters, filters or shapes need protocol 25 (Tank Yard 0.15) on both server and clients. The trigger features added in 0.15.1 (While inside, On empty, On destroy, Random signal, Set counter, Calculate, Script event, gates and the tank actions) need 0.15.1 to host a map or play it offline. Servers never send triggers to players, so 0.15.0 players can still join.

For a dedicated server, put the ZIP in persistent storage, add its path to the top-level `mods` list, and run `reload-mods`. Then `maps` lists it and `map my_arena` selects it. Joining clients receive the checked map data automatically. See [Hosting](hosting.md) and [domain setup](hosting-domain.md).

Export and playtest check dimensions, ids, text, bounds, object budgets, trigger settings, portal exits, spawn and base clearance, objectives and starting connectivity. A failed export leaves the document editable and shows the reason in Check. The schema is documented in [Modding](modding.md#maps).
