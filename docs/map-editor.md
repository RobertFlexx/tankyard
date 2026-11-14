# Arena Workshop

[Watch the tutorial](arena-workshop-tutorial.mp4) (2 min 37 s) for building, editing, map logic, plugin tools and playtesting in the workshop. The [logic gates video](logic-gates-tutorial.mp4) (1 min 42 s) shows the new gate editor and live preview. The [written logic tutorial](logic-gates-tutorial.md) builds its lever puzzle step by step with conditions, gates, watchers and sequences, and the [motions tutorial](motions-tutorial.md) builds a gatehouse with a sliding door, moving cover, chained motions, lights and particles. Plugin authors can start with the [editor plugin guide](editor-plugins.md).

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

**Pan** (key **4**) moves the view with a click and drag. Its dock contains the **Scroll** setting, pan and zoom speed controls, and **100% zoom**. Navigation preferences are remembered between editor sessions.

**Scroll** has three settings; click it to move to the next, Shift-click to go back:

- **Auto** (the default) uses the scroll events themselves: precise scrolling pans and wheel clicks zoom. macOS and Wayland identify precise events; Windows fractional wheel events are treated as precise. X11 reports both as wheel clicks. Choose Trackpad or Mouse explicitly if Auto does not match your device.
- **Trackpad**: scrolling always pans. Hold Ctrl (Command on macOS) to zoom around the pointer instead, and Shift to pan sideways.
- **Mouse**: vertical scrolling always zooms and horizontal scrolling pans.

Ctrl/Cmd-scroll zooms in every setting. Space-drag and middle-drag also pan in any mode.

**Build** places the chosen palette item:

- **Click** places one item.
- **Drag** draws one item at the dragged size. A line drawn along the grid still gets one cell of depth.
- **Swipe** (dock switch) paints a cell everywhere the pointer passes, skipping cells that already hold the same kind.
- **Right-click** any object to build copies of it, settings included. Pick a palette item to stop.

**Edit** selects and changes objects:

- **Click** selects an object; **Shift-click** adds or removes one.
- **Tab** cycles through overlapping objects under the pointer; **Shift+Tab** cycles backwards. Locked and hidden groups follow the same selection filters as clicking.
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
| 1, 2, 3, 4 | Build, Edit, Delete, Pan |
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
| Ctrl+A, Ctrl+Shift+A | Select everything shown, or everything like the selection (every light, every speed pad) |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z) | Undo, redo (48 steps) |
| Ctrl+S | Save the project |
| Ctrl+R | Run the last plugin tool again |
| Tab, Shift+Tab | Cycle overlapping objects under the pointer |
| Mouse wheel | Zoom around the pointer (Auto and Mouse scroll) |
| +, − | Zoom around the canvas center |
| Two-finger scroll | Pan (Auto and Trackpad scroll) |
| Ctrl/Cmd+scroll | Zoom around the pointer |
| Shift+scroll | Pan sideways |
| Middle-drag, Space-drag | Pan |
| F, Shift+F | Fit the arena, frame the selection |
| \\ | Hide or show the panels |
| H, G, L | Show hidden groups, group labels, logic links |
| P, Shift+P | Run or pause the logic preview, restart it |
| Logic tab: click, Shift-click, right-click a counter | Add 1, take 1 away, clear it while the preview runs |
| Hold T | Test tank at the pointer for the preview |
| F5, Shift+F5 | Playtest, or start at the pointer |

Shortcuts pause while a text box has focus. Holding a nudge key repeats it, and the whole hold undoes as one step.

The **Grid** switch cycles through 10, 20, 40, 80, 160 and 320 world units; Shift-click steps back. With **Snap** off, objects follow the pointer exactly. The step button sets the nudge distance: one unit, half a cell, one cell or four cells.

## Inspector

Click any slider's value to type an exact number, then press Enter (Esc cancels). With several objects selected, a change applies to every object that has that setting. Repeated edits to one setting undo as one step.

- **Map:** name, id, description, size, recommended players, wall style, floor, and the groups hidden when a round starts. Objects keep their relative place when the size changes.
- **Object:** position, size, group and the settings for the selected kind. A group's *starts hidden* switch is available here, so a gate needs no trip to Map.
- **Logic:** preview controls, the test tank's team, the eight counters, the groups the logic uses, a panel to send signals and defeat bosses by hand, a log of what happened, and every trigger with its summary and live state. Click a trigger to find it, or press its **Fire** button to make it act once right now, as if its moment had come. See [Logic preview](#logic-preview).
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

**Lights** brighten whatever lies under them, so they glow strongly on a dark floor and softly on a light one. Their size sets how far the light reaches, and *Flicker* makes them waver like a flame or a failing lamp (Reduced motion holds them steady). The Decor tab has Torch, Neon and Lantern presets.

**Markers** place spawn regions (neutral, team 1 or team 2), team bases and objectives. A team base needs a matching team spawn region around it.

## Groups and logic

Every block, pad, decor item and trigger can belong to a group from 1 to 32. Group 0 is always on. A hidden group's blocks stop colliding, its pads stop working, its decor disappears, and its triggers switch off. The dock's **Group** chooser puts newly built objects in a group; **Free** picks an unused one. Newly built triggers that act on a group (Toggle, Show, Hide, Reset triggers) or watch one (On shown, On hidden) use that group instead of joining it. **G** shows group numbers on the canvas, and **L** draws the logic links (see [Links](#links)).

A trigger has four parts: **when** it fires, the **filters** that apply, an optional **gate** of conditions, and the **action** it takes.

### When

- **Every:** repeats every *Every (s)* seconds, starting after *First wait*.
- **On enter / On exit:** a tank crosses into or out of the trigger's area. *Rest after acting* is the cooldown. Who and Team pick which tanks count: anyone, players only, bots only, and any team or teams 1–4. A tank that dies inside does not count as leaving.
- **While inside:** acts as soon as *Tanks needed* noticed tanks are in the area, then again every *Every (s)* seconds while they stay. Good for capture points.
- **On empty:** the last noticed tank leaves the area or is destroyed.
- **On destroy:** a noticed tank is destroyed inside the area.
- **At start:** once, *Seconds into the round* after it starts.
- **On signal:** another trigger sent a signal on its channel (1–32).
- **On counter:** its counter (1–8) reaches the threshold, from either direction.
- **On change:** its counter changes value at all, by any trigger or script. Setting a counter to the value it already has is not a change.
- **On shown / On hidden:** its group is shown or hidden, however that happened: a trigger, an *Undo after* timer or a script. The groups a round starts with don't count, so a group hidden from the start doesn't fire On hidden.
- **On boss down:** a boss is defeated anywhere in the arena. Pick one boss, or *Any boss*.
- **On arrive:** a motion of its group ends: a move, turn or fade, the trip back included. A motion cut short by another doesn't count, and neither does one with nowhere to go, such as a door told to open while it's open. See [Motions](#motions).

### Actions

- **Toggle, Show, Hide:** change a group. *Undo after* puts the group back after that many seconds; 0 keeps the change.
- **Announce:** shows text to everyone, up to 96 characters. `{1}`–`{8}` insert counters.
- **Boss:** brings the chosen boss to the trigger. A crowded map or the 16-boss limit can prevent it.
- **Signal:** fires every *On signal* trigger on that channel.
- **Random signal:** sends on one channel picked at random from a run of 2 or more, starting at *First channel*.
- **Sequence:** sends on the next channel of a run each time it acts, then starts over: with *First channel* 5 and 3 channels it sends 5, 6, 7, 5, 6…
- **Count:** adds an amount (negative subtracts) to a counter. Counters stay within ±99,999 and reset each round.
- **Set counter:** puts a counter at an exact value. Goals watching that value act as if it had been counted to.
- **Calculate:** changes a counter with a number or with another counter: add, subtract, multiply, divide (rounding down), remainder, lowest, highest, copy, a random number below the other value, or flip between 0 and 1. Dividing by zero changes nothing.
- **Shapes:** drops 1–24 shapes inside the trigger's area.
- **Repair tanks, Damage tanks:** restore or take 1–100% of maximum health from every noticed tank inside the area. Damage goes through normal damage protection.
- **Award points:** gives 1–100,000 points to every noticed tank inside the area.
- **Script event:** raises a named event for script packs (see [Modding](modding.md#map-logic)). Scripts can answer by sending signals, changing counters, showing or hiding groups and resetting triggers. Without a script pack it does nothing, and the preview only logs it.
- **Reset triggers:** makes every trigger in a group start afresh, as at the start of a round: *Once per round* triggers can act again, rest times and cooldowns end, delayed actions still waiting are dropped, timers start counting again from now and sequences go back to their first channel. Group 0 resets every trigger in the map. Counters and groups keep their state; set those with their own actions.

- **Move group, Move group to, Turn decor, Fade decor:** animate a group. See [Motions](#motions).

The tank actions act on tanks inside the trigger's own area. On a timer, signal, counter, group or boss trigger, *Who* and *Team* appear under the action to choose those tanks.

### Motions

Motion actions animate a group the way a tween does in Godot: from where it is now to a new value, over a time, along an easing curve. All of a group's blocks, pads, decor and trigger areas move together. Moving cover pushes tanks, shapes and bosses ahead of it, so a closing door shoves tanks out of the doorway rather than trapping them.

- **Move group** glides the group by an amount, from wherever it is. Each press of a Move plate takes it one step further.
- **Move group to** glides it to a place measured from where the map puts it; 0%, 0% brings it home.
- **Turn decor** turns the group's decor by a number of degrees, and **Fade decor** fades it to a percentage of its opacity. Neither affects collision.

The Object tab sets the target group, the distance as a percentage of the arena's width and height, **Over** (seconds; 0 moves at once), the **Curve** (Linear, Sine, Quad, Cubic, Quart, Quint, Expo, Circ, Back, Elastic or Bounce) and the **Mode** (Ease in, Ease out, In and out, Out and in). A graph shows the curve, with a dot that plays the motion. **Go back after** plays the motion in reverse that many seconds after it ends: doors that shut again, lifts that come back down. If the motion plays again before the group is home, as when another tank reaches a door, the wait starts over and the group still goes back to where it rested before it set off. A motion without Go back cancels the trip back and leaves the group where it ends.

With one Move trigger selected, the canvas draws its group where the motion takes it, joined by an arrow. Drag the diamond at the arrow's head to set the distance; it snaps to the grid, and Shift keeps it straight. A move may average at most one arena width a second, so dragging further lengthens **Over** as needed.

To run motions one after another, start the next from an **On arrive** trigger watching the group that moves first, or from the Then piece's signal. The [motions tutorial](motions-tutorial.md) walks through doors, moving cover and chained motions. A lift that rises, then opens its door, is a Move on the lift's group and an On arrive trigger that moves the door's group.

The logic preview plays motions too. Objects away from home show a dashed outline where the map places them, and **Fire** plays a trigger's motion at once. Motions start afresh every round.

### The Motion palette

The **Motion** tab in Build holds ready-made motions. Each acts on group 1; put the objects you want to move in that group, or pick a group in the dock first.

| Piece | What it does |
| --- | --- |
| Door | Slides group 1 up when a tank arrives, and back 4 seconds later |
| Shuttle | Glides group 1 across and back every 6 seconds |
| Piston | Punches group 1 out and pulls it back every 2 seconds |
| Lift | On signal 1, carries group 1 to 12% below its place |
| Home | On signal 2, brings group 1 back to where the map puts it |
| Drop | Drops group 1 down with a bounce as the round starts |
| Spinner | Turns group 1's decor a quarter turn every 3 seconds |
| Fader | Fades group 1's decor while tanks pass, back after 5 seconds |
| Blink | Dims group 1's decor and brings it back every 2 seconds |
| Then | Sends signal 1 each time group 1 finishes a motion |

Scripts can play motions too, with `move_map_group`, `turn_map_group` and `fade_map_group` (see [Modding](modding.md#changing-the-match)).

### Conditions and gates

**Only if** gives a trigger a gate. When the trigger fires, it checks its conditions and acts only if the gate is open. Press **Add a condition** to start one, and **Add another condition** for up to four. Each condition is a card with its own kind:

- **Counter:** a counter compares as chosen (=, ≠, <, >, ≤ or ≥) against a number, or with **Against a counter**, against another counter.
- **Group shown / Group hidden:** a group is currently shown or hidden.
- **Tanks inside:** the number of noticed tanks inside the trigger's own area, using its *Who* and *Team* filters. A Plate with *Tanks inside ≥ 2* only works for two tanks at once.
- **Round time:** whole seconds since the round started, such as *Round time ≥ 300* for overtime.

Tanks inside and Round time compare like counters, so they can also be held against a counter. **Remove** deletes a card, and **↑** moves it up one place.

With two or more conditions, **Combine** chooses how they make the gate:

| Combine | Opens when | Gate |
| --- | --- | --- |
| All | every condition holds | AND |
| Any | at least one holds | OR |
| Not all | at least one fails | NAND |
| None | none of them holds | NOR |
| Just one | exactly one holds | XOR |

With one condition, *Not all* and *None* both open when it fails, which makes a NOT gate.

A shut gate does nothing by default, and doesn't use up *Once per round*, the rest time or *Chance*. Switch on **Otherwise send a signal** to send on the *Otherwise channel* instead (it starts at the lowest channel nothing uses), which makes an if/else: the trigger acts when the gate is open and signals when it's shut. If all 32 channels are in use, free one before adding an otherwise signal. Otherwise signals count toward the 256 chained actions a tick, so they can't loop forever.

While the preview runs, every card shows **holds** or **fails**, and the gate shows how many hold and whether it is **open** or **shut**, updated live. Gated triggers show a small diamond on their canvas badge, and their tag reads like `AND of 2 → Hide` or `#1≥3 → Signal 4`. The Logic tab's trigger list spells each gate out in full.

### Timing

For every trigger:

- *Chance* is the percent chance it acts when it fires, and can make a coin flip.
- *Delay before acting* waits that long after the trigger fires.
- *Once per round* stops it after its first action, until a Reset triggers action rearms it.

Signals, counters and group changes can chain: a Show action can set off an On shown watcher, which counts, which sets off a Goal. Each tick allows 256 chained actions, so a loop stops instead of freezing the server. Map changes and new rounds reset the logic.

Triggers keep their listeners in lookup tables built when a round starts, so a signal or counter change only visits the triggers listening for it. A map with all 128 triggers costs no more per tick than the triggers that actually fire.

### Links

With **L** on, the canvas draws a wire between every pair of linked triggers, and from triggers to the objects they act on:

- **Violet:** a signal. A thinner violet wire is an otherwise signal.
- **Amber:** a counter one trigger changes and another watches or checks.
- **The group's color:** a group one trigger switches and another watches or checks, or the group's objects.
- **Red:** a Reset triggers action and the triggers it rearms.

### Logic preview

Press **P** to run the map's triggers in the editor with the same engine servers use. Timers count, groups switch on the canvas, and triggers flash when they act. The Logic tab shows everything:

- **Play/Pause**, **Step** (one tick, 1/60 s, while paused), **Restart** and a speed of ×0.25 to ×4. Slow motion helps to follow a fast chain.
- How many delayed actions are waiting.
- **Counters:** click one to add 1, Shift-click to take 1 away and right-click to clear it. Watchers and goals hear the change as if a trigger made it.
- **Groups:** every group the logic uses. Lit groups are shown; click one to show or hide it, as a script would. Watchers and gates see the change.
- **Send:** pick a *Signal channel* and press **Send signal**. If any trigger waits for a boss, **Defeat** pretends the chosen boss was just defeated.
- **What happened:** counters, announcements, bosses, shapes, repairs, damage, points and script events.
- **Triggers:** each trigger's summary with live chips: *spent* for a once-per-round trigger that acted, *next 4.2 s* for a timer, *next sends 7* for a sequence, *open* or *shut* for a gate. **Fire** makes a trigger act once right now, starting the preview if needed.

**Hold T** to drive a test tank at the pointer through plates and exits; the Logic tab sets its team and whether it counts as a bot. **Shift+P** restarts. Editing a trigger restarts the preview by itself.

### The Gates palette

The **Gates** tab in Build holds ready-made logic. Each piece starts on counters 1 and 2, signal 1 and group 1; change them in the Object tab, or pick a group in the dock first. Hover a tile to see what it does.

| Piece | What it does |
| --- | --- |
| Gate | Passes signal 1 on as signal 2 while counter 1 is at least 1 |
| AND, OR, XOR | Pass signal 1 on as signal 3 while counters 1 and 2 are both on, either is on, or just one is on |
| If/else | On signal 1, sends signal 2 if counter 1 is on, otherwise signal 3 |
| Lever | Flips counter 1 between 0 and 1 when a tank enters |
| Coin flip | Toggles a group half of the time, every 5 seconds |
| Dice | Sends one of signals 1–3 at random every 15 seconds |
| Sequence | Sends signals 1, 2 and 3 in turn, one every 3 seconds |
| Display | Announces counter 1 every time it changes |
| Alarm | Announces when group 1 is hidden, however it happened |
| Reward | Hides group 1 when any boss is defeated |
| Reset | Puts counter 1 back to 0 when the area empties |
| Rearm | On signal 1, makes group 1's triggers start afresh |
| Crowd | Hides group 1 for 8 seconds once two tanks stand on it together |
| Clock | Announces every 30 seconds once the round passes 5 minutes |

The **Logic** tab keeps the single-step pieces: Timer, Plate, Exit pad, Start, Sender, Receiver, Counter, Goal, Shapes, Boss, Capture, Bonus, Medic, Trap and Script.

### Recipes

A pressure gate:

1. Choose a free group in the dock (**Free**), then draw a wall. Switch on *Group N starts hidden* in Object.
2. From **Logic**, place a **Plate** beside it. It already shows that group for 5 seconds when a tank enters.
3. Press **L** to check the wire, then hold **T** over the plate with the preview running.

A lever door, with one gate:

1. Choose a free group and draw the door. From **Gates**, place two **Levers**; set the second one's counter to 2.
2. Place an **On change** watcher for each counter (When: *On change*, Action: *Signal*, channel 1). Any lever moving now sends signal 1.
3. Place an **AND** from Gates. Change its action from Signal to *Hide*, targeting the door's group.
4. Switch on **Otherwise send a signal** with channel 2, and add a Receiver on 2 that shows the door. Now the door opens only while both levers are on, and closes again when either turns off.
5. Try *Any (OR)* or *Just one (XOR)* in Combine for different puzzles, and add labels reading `Lever {1}` and `Lever {2}`.

The bundled Logic Kit's **Lever bank** tool builds this whole circuit for two to four levers in one step.

A capture point:

1. Place a **Capture** trigger over the point. While a tank is inside, it adds 1 to counter 1 every second.
2. Place a **Goal** watching counter 1 with a threshold of 10 and an Announce action, such as `Point taken!`.
3. Add a **Reset** from Gates over the same area so leaving it starts the count again, and a label reading `Hold {1}/10`.

A two-tank vault:

1. Draw the vault wall in its own group. Place a **Crowd** plate from Gates in front of it and set it to hide the vault's group.
2. Two tanks must stand on the plate together to open it for 8 seconds. Raise *Tanks inside* to 3 for a team of three, or set *Team* so only one side counts.
3. Add an **Alarm** from Gates that announces when the vault's group is hidden.

A repeating wave:

1. Place a **Sequence** from Gates. Every 3 seconds it sends 1, 2, 3, then starts over.
2. Add a Receiver for each channel: Shapes on 1, Toggle a hazard pad's group on 2, and Announce `Wave {1}` on 3 with a Count on counter 1 beside it.
3. Add a **Clock** that announces overtime, and a gate on the Shapes receiver with *Round time < 300* so shapes stop late in the round.

A boss reward:

1. Place a **Boss** from Logic to bring a boss partway through the round.
2. Place a **Reward** from Gates. Pick the boss in *Boss*, and target the group of the wall around a bonus area.
3. In the preview, press **Defeat** in the Logic tab to test it without a fight.

A puzzle that resets:

1. Put a puzzle's once-per-round switches in their own group, say group 5, by choosing it in the dock before placing them.
2. Place a **Rearm** from Gates and set its group to 5. When signal 1 arrives, every switch in the group can be pressed again.
3. Send signal 1 from a reset plate or a timer, and add a Set counter action to put the puzzle's counter back to 0.

Avoid closing solid cover on spawns or objectives. Map checks cover the starting layout and connectivity; they can't prove that every combination of groups stays passable.

### Logic checks

The Check tab notes logic that can never work as built:

- a trigger waits for a signal nothing sends, or sends signals nothing receives (including otherwise signals and every channel of a Random signal or Sequence);
- a watcher or condition reads a counter nothing changes, or a group nothing shows or hides;
- a Reset triggers action targets a group with no triggers;
- triggers signal each other in a loop with no delay, gate or chance, which the arena cuts short every tick.

Notes never block a playtest; scripts can send signals and change counters and groups the checks can't see.

## Workshop plugins

Packs can add tools and checks to the workshop. Enable a plugin pack on **Mods**, review any findings, then open the **Tools** tab. Each tool shows its settings and a Run button. A run changes the map as one undo step and usually selects what it made; **Ctrl+R** runs the last tool again with the same settings. Plugin checks add amber notes to the Check tab and never block a playtest or export.

The bundled **Workshop Toolkit** adds Repeat, Ring of blocks, Arena frame, Snap to grid and Four-way mirror, and a check for gaps too narrow to drive through. The bundled **Logic Kit** builds logic for you: **Switch door** turns the selected blocks into a door with switches that open it when all or any are pressed, **Lever bank** wires two to four levers to an AND, OR or XOR gate that opens the selected blocks, **Countdown** counts a free counter down from the start of the round and announces a message at zero, and **Sequence** makes the selected triggers act one after another when a tank enters a new plate. Its **Unread counters** check notes counters that change but that nothing reads. Plugin packs only change the editor, so enabling one never marks a match as modded. After editing a plugin's scripts, press **Reload plugins**. A tool's result passes the same checks as an import before the workshop keeps it, and a tool that errors or runs too long changes nothing. The full plugin guide covers pack creation, installation, tools, parameters, checks, helpers and debugging. [Editor plugin guide](editor-plugins.md)

## Particle generator

Place an emitter from **Decor**. Its Object tab has ten presets (Embers, Rain, Dust, Sparks, Fireflies, Smoke, Snow, Glitter, Fountain and Pulse) and every setting: shape, where particles start, rate, lifetime, speed, direction, spread, gravity, drag, start and end size, spin, wobble, twinkle, fade, glow and both colors. Direction uses screen coordinates: 0° right, 90° down.

- **Shape:** dot, square, spark, ring, triangle, soft (a dot with a soft edge, for smoke, mist and glows) or star.
- **Emit from:** anywhere in the emitter (Fill), inside the ellipse it encloses (Round), along its outline (Edge) or from its center.
- **Wobble** sways each particle sideways as it travels, for smoke and fireflies. **Twinkle** makes each one flicker.
- **Glow** adds particles' light to what lies beneath them, like embers or sparks.

Particles are drawn from seeds, so they cost the server nothing and send no network traffic. A map has at most 48 emitters, each with at most 240 live particles, and the client draws at most 1,024 map particles, scaled by the Effects setting. Reduced motion hides map particles and freezes spin, pulse and conveyor marks.

## Rules

The Rules tab sets movement, acceleration, coasting, damage, health, regeneration, experience, score and respawn time for the playtest and the exported pack. Standard, Arcade, Endurance and Drift are starting profiles. These are map-pack rules, not permissions or server security settings.

An authorized host can change the same rules during a match with **Server → Rules**. Health changes keep each tank's health percentage, and players keep their upgrades and scores. Customized matches do not earn normal arena records.

## Files and deployment

The workshop folder is `workshop/` inside Tank Yard's data directory. **Files → Open folder** reveals it. It contains:

- `draft.json`: the recovery draft.
- `projects/<map-id>.json`: named editable documents.
- `exports/<map-id>-<timestamp>.zip`: portable exports.
- `prefs.json`: panel sizes, canvas switches, navigation settings, the palette tab and item, the inspector tab, the logic preview's speed, and where you were looking in the map you had open.

Each export contains `mod.json` and `editor.json`. Gameplay uses the manifest, and the editor document lets others keep editing. The ZIP holds no compiled code. Maps that use conveyors, portals, signals, counters, filters or shapes need protocol 25 (Tank Yard 0.15) on both server and clients. The trigger features added in 0.15.1 (While inside, On empty, On destroy, Random signal, Set counter, Calculate, Script event, single-condition gates and the tank actions) need 0.15.1 to host a map or play it offline. The logic added in 0.16.0 (On change, On shown, On hidden, On boss down, Sequence, Reset triggers, the Tanks inside and Round time conditions, several conditions in one gate and otherwise signals) needs 0.16.0 to host a map or play it offline; older servers refuse such a map with a clear error. Tank Yard 1.0 adds motions, light flicker and the new particle settings. It uses protocol 26, so 1.0 servers and players play only with each other, and maps saved or exported from 1.0 need 1.0 to open.

For a dedicated server, put the ZIP in persistent storage, add its path to the top-level `mods` list, and run `reload-mods`. Then `maps` lists it and `map my_arena` selects it. Joining clients receive the checked map data automatically. See [Hosting](hosting.md) and [domain setup](hosting-domain.md).

Export and playtest check dimensions, ids, text, bounds, object budgets, trigger settings, portal exits, spawn and base clearance, objectives and starting connectivity. A failed export leaves the document editable and shows the reason in Check. The schema is documented in [Modding](modding.md#maps).
