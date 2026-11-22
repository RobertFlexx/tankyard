# Arena Workshop

[Watch the tutorial](arena-workshop-tutorial.mp4) (2 min 37 s) for building, editing, map logic, plugin tools and playtesting in the workshop. The [logic gates video](logic-gates-tutorial.mp4) (1 min 42 s) shows the gate editor and live preview, and the [motions video](motions-tutorial.mp4) (2 min 15 s) covers sliding doors, curves, chained motions, lights and particles. Follow the [written logic tutorial](logic-gates-tutorial.md) to build a lever puzzle, the [motions tutorial](motions-tutorial.md) to build a gatehouse or the [physics and lighting tutorial](physics-tutorial.md) and [video](physics-tutorial.mp4) to build a dark yard with a push door, a crate puzzle and a ferry. The [menus and waypoints tutorial](ui-tutorial.md) and [video](ui-tutorial.mp4) tour the UI designer and Arcade Hall shop, and the [widget styles tutorial](widget-styles-tutorial.md) and [video](widget-styles-tutorial.mp4) show the preset looks and style options. Plugin authors can start with the [editor plugin guide](editor-plugins.md).

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
| Click a selected object again | Select the next object under the pointer, wrapping round to the top |
| Tab, Shift+Tab | Cycle overlapping objects under the pointer |
| , and . | Go down or up a [layer](#layers) (Shift: skip empty layers) |
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

## Layers

Layers keep a busy map workable. Every object sits on a layer from 0 to 99, and new objects land on the layer you're editing.

- The **layer bar** at the lower left of the canvas shows the current layer. Its arrows step through the layers, and clicking its name switches between that layer and **All layers**.
- Point at the layer bar to open a strip of every layer in use, each with a small bar showing how much it holds. Click one to go straight there.
- **,** and **.** go down and up a layer. Hold Shift to skip layers with nothing on them.
- While you edit one layer, everything else fades and can't be selected, so you can reach what's underneath without moving what's on top.
- The **Layer** field in the Object tab moves the selection to another layer.
- Copies keep their layers. While you edit one layer, pasted objects land on it.

Layers are only for the workshop. They're saved in your project, never change how a map plays, and don't affect draw order. Use **[** and **]** for that.

To reach an object under another without layers, click it. A click takes the smallest object under the pointer, so a trigger sitting on a spawn region or a floor tile is one click away. Clicking the selected object again moves to the next one there, smallest to largest, and wraps round. The tooltip says how many are stacked. A click and a drag stay separate, so dragging still moves what you selected.

## Inspector

Click a slider's value box to type an exact number. **Enter** applies it, **Escape** cancels, and **Tab / Shift+Tab** applies it and moves to the next or previous number. The inspector scrolls to keep the focused field visible. A valid edit also applies when you click elsewhere; invalid entries stay out of the map.

Simple calculations work: `4000/2`, `(120+40)*2`, or `+=50`, `-=10`, `*=2`, `/=2` relative to the value you started editing. Values must fit the range shown by the control. Typed floating-point values can be finer than the slider's drag step, including subunit positions and sizes. Counts, channels, colors and other whole-number settings still round to whole numbers. Use Up/Down to step, Shift for larger steps or Alt for smaller ones. Hold Shift while dragging a slider for fine adjustment.

Group and team numbers have the same entry boxes. Color controls accept `#RGB` or `#RRGGBB` as well as swatches and RGB sliders. Drag the inspector's scrollbar, or point inside it and use Page Up, Page Down, Home or End when no field has focus.

With several objects selected, a change applies to every object that has that setting. **Space X evenly / Space Y evenly** spreads their centers between the two outermost objects. **Match widest / Match tallest** copies the largest selected dimension to the other rectangles. Each arrangement is one undo step; fixed-size point markers keep their size. Each committed numeric edit also undoes separately; a continuous slider drag remains one step.

- **Map:** name, id, description, size, recommended players, wall style, floor, [lighting](#darkness-and-light), [paths](#paths) and the group list. Objects keep their relative place when the size changes. The group list shows every group in use with its name and what it holds (blocks, pads, decor, triggers, and whether tanks can push it). **Hidden** makes a group start the round out of sight, **Lock** stops it being selected on the canvas while you work around it, and **Select** selects the whole group and centers the view on it.
- **Object:** position, size, group and the settings for the selected kind. A group's *starts hidden* switch is available here, so a gate needs no trip to Map. A trigger also lists its **Connections**: what it sets off and what sets it off. Click a row to select that trigger and center on it.
- **Logic:** preview controls, the test tank's team, the counters, the groups the logic uses, a panel to send signals and defeat bosses by hand, a log of what happened, and every trigger with its summary and live state. Click a trigger to find it, or press its **Fire** button to make it act once right now, as if its moment had come. See [Logic preview](#logic-preview).
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
- **Launch pad:** flings tanks along its direction at `900 × strength` units a second, then lets them coast for about half a second before steering takes over again. A tank already flying that way at most of that speed is left alone, so a pad launches once rather than on every frame a tank sits on it. Sideways motion is kept, so a tank crossing at an angle curves.
- **Ice:** tanks keep `22%` of their grip (less at higher strength), so they get going slowly, turn wide and slide on after letting go.
- **Gravity well:** draws tanks toward its middle at `130 × strength` units a second, up to twice that near the center. Driving straight out still works; drifting past costs time.

Overlapping speed and mud pads multiply, and the final factor is clamped to 0.3–2.5. Overlapping conveyors add up to 600 units a second, and overlapping launch pads up to 1,800. Repair and hazard rates add; shelter pads multiply. Speed, conveyor and launch effects run in shared code, so movement predicts smoothly online.

**Decor** includes tiles, stripes, lights, spotlights, beams, grids, rings, discs, triangles, hexagons, arrows, labels and particle emitters. It has color, opacity, rotation, spin, pulse and an *Above tanks* switch. It never changes collision. A label can show counters: `{1}` to `{255}` are replaced with that counter's value, in any of the [counter formats](#counters).

**Near a tank** makes decor respond to the player's own tank within a distance (220 units unless you set **Within**): **Fade away** clears it, for roofs and canopies that hide what is under them until you drive in; **Appear** shows it, for hidden signs; **Grow** swells it; and **Glow** brightens it. Each player sees their own tank's effect, so a roof one player has driven under stays shut for everyone else. The Decor tab's **Roof** piece is a tile that is already above tanks and fades.

**Lights** brighten whatever lies under them, so they glow strongly on a dark floor and softly on a light one. Their size sets how far the light reaches, and *Flicker* makes them waver like a flame or a failing lamp (Reduced motion holds them steady). The Decor tab has Torch, Neon and Lantern presets. A **Spotlight** throws a cone of light from one end of its rectangle to the other; turn it to aim it. A **Beam** is a straight shaft of light along its length.

### Darkness and light

**Lighting** in the Map tab darkens the whole arena toward a color, up to 95%. Darkness hides the floor, cover and shapes alike; light cuts through it. Lights, spotlights and beams light the floor beneath them, and so do particle effects with **Glow** on, hazard, portal, launch and repair pads, every tank (a small light around the hull) and every shot. A spotlight also leaves a soft pool where its beam lands. A **Lighting** trigger changes the darkness during a round, over a time and along a curve, and **Go back after** returns it to what it was, so a quick change to 0% with a short Go back is a lightning flash. With **Reduced motion** on, players see sudden changes ease in over about half a second.

In the editor the canvas shows the darkness at half strength so you can still build, and at full strength while the logic preview runs. On computers without shader support the darkness is a flat veil with soft light spots.

**Markers** place spawn regions (neutral, team 1 or team 2), team bases and objectives. A team base needs a matching team spawn region around it.

## Groups and logic

Every block, pad, decor item and trigger can belong to a group from 1 to 32. Group 0 is always on. A hidden group's blocks stop colliding, its pads stop working, its decor disappears, and its triggers switch off. The dock's **Group** chooser puts newly built objects in a group; **Free** picks an unused one. Newly built triggers that act on a group (Toggle, Show, Hide, Reset triggers) or watch one (On shown, On hidden) use that group instead of joining it. **G** shows group numbers on the canvas: where several objects of one group sit together, one number stands for them, and numbers make way for trigger tags. Point at an object or select it to see its own. **L** draws the logic links (see [Links](#links)).

A trigger has four parts: **when** it fires, the **filters** that apply, an optional **gate** of conditions, and the **action** it takes.

### When

- **Every:** repeats every *Every (s)* seconds, as often as every 0.05 seconds, starting after *First wait*.
- **On enter / On exit:** a tank crosses into or out of the trigger's area. *Rest after acting* is the cooldown. Who and Team pick which tanks count: anyone, players only, bots only, and any team or teams 1–4. A tank that dies inside does not count as leaving.
- **While inside:** acts as soon as *Tanks needed* noticed tanks are in the area, then again every *Every (s)* seconds while they stay. Good for capture points.
- **On empty:** the last noticed tank leaves the area or is destroyed.
- **On destroy:** a noticed tank is destroyed inside the area.
- **At start:** once, *Seconds into the round* after it starts.
- **On signal:** another trigger sent a signal on its channel (1–255).
- **On counter:** its counter (1–255) reaches the threshold, from either direction.
- **On change:** its counter changes value at all, by any trigger or script. Setting a counter to the value it already has is not a change.
- **On shown / On hidden:** its group is shown or hidden, however that happened: a trigger, an *Undo after* timer or a script. The groups a round starts with don't count, so a group hidden from the start doesn't fire On hidden.
- **On boss down:** a boss is defeated anywhere in the arena. Pick one boss, or *Any boss*.
- **On touch / On release:** a block of the watched group starts to overlap the trigger's area, or the last one stops overlapping it. A crate pushed onto a plate, a door sliding shut or a ferry reaching its dock all count. Together they make a pressure plate that only cover can press.
- **On input change:** a counter or group that the trigger's conditions read changes, and once as the round starts. With **Gate to counter** it's a live logic gate that needs no clock. See [Live gates](#live-gates).
- **On arrive:** a motion of its group ends: a move, turn, fade, tint, size change or an animation easing in or out, the trip back included. A group on a path arrives at the path's end, not at each point. Several motions ending for the same group in one simulation tick produce one event. A motion cut short by another doesn't count, and neither does one with nowhere to go, such as a door told to open while it's open, or one that a Stop action halts. See [Motions](#motions).

### Actions

- **Toggle, Show, Hide:** change a group. *Undo after* puts the group back after that many seconds; 0 keeps the change.
- **Announce:** shows text to everyone, up to 96 characters. `{1}`–`{255}` insert counters (see [Counters](#counters)); `{cause}` names the tank that set it off, or is empty when there is none.
- **Boss:** brings the chosen boss to the trigger. A crowded map or the 16-boss limit can prevent it.
- **Signal:** fires every *On signal* trigger on that channel.
- **Random signal:** sends on one channel picked at random from a run of 2 or more, starting at *First channel*.
- **Sequence:** sends on the next channel of a run each time it acts, then starts over: with *First channel* 5 and 3 channels it sends 5, 6, 7, 5, 6…
- **Count:** adds an amount (negative subtracts) to a counter. Counters stay within ±1,073,741,823 and reset each round.
- **Set counter:** puts a counter at an exact value. Goals watching that value act as if it had been counted to.
- **Gate to counter:** stores whether the trigger's gate is open in a counter, as 1 or 0. The gate never stops this action. See [Live gates](#live-gates).
- **Calculate:** changes a counter with a number or with another counter: add, subtract, multiply, divide (rounding down), remainder, lowest, highest, copy, a random number below the other value, flip between 0 and 1, and (1.7) the bit math AND, OR, XOR, shift left and shift right, and power. Dividing by zero changes nothing, and so does a negative power. Either counter can be **moved along by another counter** for lists of counters; see [Counters](#counters).
- **Shapes:** drops 1–24 shapes inside the trigger's area.
- **Repair tanks, Damage tanks:** restore or take 1–100% of maximum health from every noticed tank inside the area. Damage goes through normal damage protection.
- **Award points:** gives 1–100,000 points to every noticed tank inside the area.
- **Script event:** raises a named event for script packs (see [Modding](modding.md#map-logic)). Scripts can answer by sending signals, changing counters, showing or hiding groups and resetting triggers. Without a script pack it does nothing, and the preview only logs it.
- **Reset triggers:** makes every trigger in a group start afresh, as at the start of a round: *Once per round* triggers can act again, rest times and cooldowns end, delayed actions still waiting are dropped, timers start counting again from now and sequences go back to their first channel. Group 0 resets every trigger in the map. Counters and groups keep their state; set those with their own actions.

- **Move group, Move group to, Turn decor, Fade decor:** animate a group. See [Motions](#motions).
- **Tint decor, Scale decor, Animate decor, Stop group:** color, resize, animate or halt a group's decor. See [Effects](#effects).
- **Shake screen:** shakes the views of the noticed tanks inside the area. See [Effects](#effects).
- **Play sound:** plays one of eight sounds at the trigger. See [Effects](#effects).
- **Shove body, Fling tanks, Blast, Zoom view, Lighting, Play path:** push a body, throw tanks, change the view or the darkness, or send a group along a path. See [Physics](#physics).
- **Show UI, Hide UI:** put a designed widget on players' screens, such as a message, a counter, a timer or a menu of choices, and take it away again. See [UI](#ui).

The tank actions and Shake screen act on tanks inside the trigger's own area. On a timer, signal, counter, group or boss trigger, *Who* and *Team* appear under the action to choose those tanks.

*Who* can also be **Tank that set it off**: only the tank behind what happened, wherever it is. That's the tank that crossed an On enter, On exit, On empty or On destroy area, the first tank inside a While inside area, or the player who picked a choice. It carries through signals, delays, counters and group watchers, so a plate can send a signal to a trigger far away that rewards just the tank that stepped on it. A timer, an At start trigger or a script's signal has no tank behind it, so such a trigger reaches nobody. Area triggers with this setting watch for any tank.

### Counters

A map has 255 counters, numbered from 1. Each starts the round at 0 and stays within ±1,073,741,823; a result past that stops at the limit. Count, Set counter and Calculate change them, On counter and On change watch them, and gates compare them.

**Counter text.** Announcements, decor labels, widget words and board cells fill in counter fields, and keep them up to date while they show:

| Field | Shows |
| --- | --- |
| `{3}` | counter 3 |
| `{3:,}` | commas between thousands: 1,234,567 |
| `{3:04}` | at least 4 digits, with zeros in front: 0042 |
| `{3:b}`, `{3:b8}` | binary, with at least 8 digits in the second form: 00000101 |
| `{3:x}`, `{3:x4}` | hexadecimal, with at least 4 digits in the second form: 00FF |
| `{3\|2\|1:4}` | counters 3, 2 and 1 as 4-digit parts of one number, the most significant first. With 12, 0 and 345 in them it shows 120000345, and `{3\|2\|1:4,}` shows 120,000,345 |

A joined field takes two to eight counters with one to nine digits each, and the first part that isn't 0 gives the sign. Anything in braces that isn't one of these, such as `{name}` outside a widget, stays as written.

**Big numbers.** One counter holds just over a billion. For more, keep a number in parts of, say, four digits a counter: carry between them with Divide and Remainder by 10,000, and show them with `{3|2|1:4}`.

**Lists of counters.** Calculate's **Move along by a counter's value** adds another counter's value to the counter's number, so one trigger can work through a list: with counter 10 at 2, counter 20 moved along by counter 10 is counter 22. The other counter can move along the same way. A number that lands outside 1 to 255 does nothing.

**Bits.** AND, OR and XOR work on the bits of the two values, shift left and shift right double or halve the counter that many times, and power raises it to the other value. With them one counter holds 30 on/off switches: OR 4 turns on bit 3, AND −5 turns it off, XOR 4 flips it, shift right by 2 and Remainder 2 read it, and a board light set to *One bit of a counter* shows it.

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

### Effects

Follow [Animate and effects](effects-tutorial.md) to build a floating lantern, combine animation styles, trigger an arrival chime and flash, freeze a group and add a launch pad. The bundled [Effects Lab](../mods/effects_lab) is the finished map.

Effects change how a group looks and how the arena feels without touching collision. Blocks and pads keep their shape and size; only decor, lights and particle emitters change. Tint, Scale and Animate play like motions, with **Over**, **Curve**, **Mode** and **Go back after**.

- **Tint decor** mixes a color into the group's decor, lights and particles. 100% paints them over completely; 0% removes the tint. A short Over and a short Go back after make a flash.
- **Scale decor** draws the group's decor and emitters at 0–400% of their placed size, about their own centres. Particles come out larger or smaller to match.
- **Animate decor** plays a looping animation on the group's decor:

  | Style | Size | What it does |
  | --- | --- | --- |
  | Bob | 0–400 units | Floats up and down |
  | Sway | 0–180 degrees | Rocks back and forth about its centre |
  | Throb | 0–100% | Swells and shrinks |
  | Shake | 0–400 units | Trembles |
  | Orbit | 0–400 units | Circles around where it rests |

  *Times a second* sets the pace, from 0.05 to 12. The five styles play together, so one group can bob and sway at once. Size 0 settles a style; with Over, it eases out. Changing the pace of a running animation keeps it where it is in its cycle, so it never jumps.
- **Stop group** halts everything the group's motions are doing, where it is: moves, turns, fades, tints and sizes stop part way, animations freeze mid-pose, and any trip back is cancelled. Nothing that was stopped arrives, so On arrive triggers stay quiet. Play another motion to start the group again.
- **Shake screen** shakes the views of the noticed tanks inside the area, at a strength from 1 to 100% for 0.1 to 10 seconds. A stronger shake replaces a weaker one. Players can turn shaking down with **Camera shake** in Settings, and Reduced motion turns it off.
- **Play sound** plays Chime, Bell, Alarm, Switch, Whoosh, Rumble, Fanfare or Thump at the middle of the trigger's area, at 1–100% volume. Players nearby hear it at full volume and it fades out a little past the edge of the screen. It uses each player's Effects volume. Bursts of the same sound within 25 milliseconds share a cue, with the loudest volume retained. Up to twelve cues overlap; additional sounds in a full mix are skipped, so sounds already playing can finish.

Reduced motion holds animations still but keeps tints and sizes, since those carry meaning: a red door is still red. Tint, Scale and Animate need Tank Yard 1.1.0 or later on the server and every player.

### The Effects palette

The **Effects** tab in Build holds ready-made effects. Like the Motion pieces, each acts on group 1 unless you pick a group in the dock first.

| Piece | What it does |
| --- | --- |
| Float | Bobs group 1's decor gently from the start of the round |
| Sway | Rocks group 1's decor slowly from the start of the round |
| Throb | Makes group 1's decor pulse in size from the start of the round |
| Orbit | Circles group 1's decor slowly from the start of the round |
| Quiver | Makes group 1's decor tremble for a second when a tank arrives |
| Flash | Flashes group 1's decor white when a tank arrives |
| Alarm | Flashes group 1's decor red every 1.2 seconds |
| Dye | On signal 1, tints group 1's decor cyan |
| Grow | Grows group 1's decor with an overshoot when a tank arrives, back after 2 seconds |
| Stop | On signal 1, halts whatever group 1 is doing |
| Quake | Shakes the views of tanks that enter its area, at most every 3 seconds |
| Chime | Plays a chime when a tank arrives |
| Siren | Plays the alarm on signal 1 |

The logic preview plays effects too. With the test tank inside a Shake screen trigger's area, the canvas shakes while the preview runs, and Play sound triggers play through your speakers.

Scripts have the same effects: `tint_map_group`, `scale_map_group`, `animate_map_group`, `stop_map_group`, `shake_view` and `play_sound` (see [Modding](modding.md#changing-the-match)).

### Physics

**Pushable bodies.** Select any block of a group and turn on **Pushable** in the Object tab, and the group's blocks become one solid body that tanks push by driving into it. Its decor, pads and trigger areas ride along. The body settings:

- **Slides:** any way, like a crate; left and right only; or up and down only, like a sliding door.
- **Reach:** how far it can go from where the map places it, separately left, up, right and down, as a percentage of the arena.
- **Weight:** 0.25 to 8. One tank pushes a body of weight 1 at its own speed; two tanks together push weight 2 as fast.
- **Spring back:** how fast it returns to its place once no one pushes. With a spring and one-way reach, a body is a push door that swings shut behind you.
- **Friction** stops it after a shove, **Bounce** keeps some of its speed when it hits the end of its reach or other cover, and **Shots push it** lets bullets knock it about, harder for heavier hits.

A body stops at other cover and never crushes a tank against a wall: it halts instead. Its motion is shared with players in each snapshot and predicted between them, so pushing it feels immediate online.

**Triggers that move things:**

- **Shove body** gives a body a push at a direction and speed, as if a tank had rammed it. Heavier bodies move less. The Check tab notes a Shove whose group isn't pushable.
- **Fling tanks** throws the noticed tanks inside the area at a direction and speed, up to 3,000 units a second. They coast for a moment, like off a launch pad.
- **Blast** throws tanks inside the area away from its middle, or with a negative speed pulls them in.
- **Zoom view** widens (below 100%) or narrows (above 100%) the view of the noticed tanks inside for up to 60 seconds, then eases back. **Hold until the next zoom** keeps it until another Zoom view changes it, so a pair of triggers can zoom out when tanks enter a hall and back in at its door. **Change over** sets how long the view takes to change, from 0.1 to 10 seconds (half a second unless you change it), along a curve such as *Sine* or *Back*. **Turn toward the area** moves the camera that far toward the middle of the trigger's area, from 0% (stay on the tank) to 100% (center on the area), to show a vault opening or a boss arriving. A new zoom takes over from wherever the view is, without a jump. A tank that is destroyed gets its usual view back, and servers send everything a zoomed-out or turned view can see.
- **Lighting** changes the arena's darkness and its color. See [Darkness and light](#darkness-and-light).
- **Play path** sends a group along one of the map's paths.

### Paths

A path is a custom animation: a list of points a group travels through, each with its own timing and curve. Add paths in the Map tab under **Paths**. Each has a name and a **Repeat**: **Once** stops at the last point, **Loop** carries on from the last point back to the first, and **There and back** turns around at each end.

Each point sets where the group is, as an offset from where the map places it, **Seconds to get here**, **Wait here**, how far to turn the group's decor on the way, and the curve. Because points are offsets, one path can move any group. Long hops take longer rather than break the speed limit of one arena width a second. A map holds up to 32 paths of up to 64 points.

A **Play path** trigger starts a path on a group, and the group's blocks push tanks out of the way as it goes, the same as a Move. A Move, Move to or Stop on the group takes it off the path. When a Play path trigger is selected, or while you edit a path in the Map tab, the canvas draws the path from where the group rests through each numbered point, with a dot that rides it at its own pace and a ghost of the group at the point you're editing.

### The Physics palette

The **Physics** tab in Build holds ready-made pieces. Pieces that act on a group use group 1 unless you pick a group in the dock first.

| Piece | What it does |
| --- | --- |
| Fling pad | Throws tanks that enter it 900 units a second, straight up |
| Blast | Throws tanks outward from its middle every 4 seconds |
| Vortex | Tugs tanks inside toward its middle twice a second |
| Crate plate | Hides group 2 while any block of group 1 sits on it |
| Plate release | Shows group 2 again when group 1's last block leaves it |
| Shove | On signal 1, shoves body group 1 600 units a second |
| Path | Sends group 1 along path 1 as the round starts |
| Zoom out | Widens the view of tanks that drive in to 60% for 6 seconds |
| Nightfall | Darkens the arena to 75% over 8 seconds, a minute into the round |
| Lights on | On signal 1, brings full light back over 1.5 seconds |
| Lightning | Every 9 seconds, flashes a dark arena to full light for a moment |

The logic preview plays paths and lighting changes on the canvas. Shoves, flings, blasts and zooms appear in its log, with the tanks they reach; press **F5** to feel them in a playtest.

Scripts can do the same with `shove_map_body`, `fling_tank`, `play_map_path`, `set_map_lighting` and `map_body` (see [Modding](modding.md#map-logic)).

### UI

A **Show UI** trigger puts a widget on players' screens or floats it over the arena. A widget can have a title, a line of text with an icon, a bar and up to six choices, or (1.7) a [board](#boards) of buttons, screens, digits, lights and meters, and each part is optional. The inspector draws a picture of the screen above the settings, with a tank in the middle for scale, and replays the entrance whenever you change something. **Replay entrance** plays it again.

- **Slot:** 1 to 32. Each slot holds one widget per player, and showing another widget in the same slot replaces it. Give a running score its own slot so a message in another slot never covers it.
- **Words:** a title of up to 48 bytes and text of up to 120. `{1}`–`{255}` show counters in any of the [counter formats](#counters), `{left}` the seconds before the widget goes, `{time}` the round's clock, and `{name}` and `{score}` the player's own. `{cause}` names the tank that set it off, including through signals and delays. They update by themselves while the widget shows.
- **Looks:** twelve preset looks: **Theme** (the player's own colors), **Neon**, **Arcade**, **Parchment**, **Terminal**, **Royal**, **Glass**, **Hazard**, **Sticker**, **Frost**, **Ember** and **Jungle**. A look sets the colors, shape, border, fill, lettering, bar and buttons in one click and keeps the words, place and choices. You can change any of them afterwards.
- **Shape, border and fill:** panels and cards can be **Rounded**, **Square**, have **Cut corners**, lean (**Slanted**), end in a point (**Tag**) or have notched ends (**Ribbon**). The border is a **Line**, **None**, **Thick**, **Double**, **Dashed** or **Corners** (brackets). The fill is **Solid**, a **Gradient**, see-through **Glass**, or **Stripes**, a **Grid** or **Dots** in an optional fill color.
- **Lettering:** **Auto**, **Clean**, **Shadow**, **Block** (a painted shadow in the accent), **Outline** or **Glow**, with **Capital letters** and an optional title color. Bars can be a **Capsule**, **Flat**, **Thin**, **Striped**, **Glow** or a row of **Pips**, and choices a **Capsule**, **Box**, **Outline** or **Underline**.
- **Look:** **Plain** text with a soft shadow, a **Panel**, a **Card** with a colored stripe, a **Banner** across the whole screen, or a **Pill**. Pick one of 20 icons, the text size and the panel's opacity. **Glow in the accent color** surrounds the widget with a soft glow, and gives plain text a halo. Without colors of its own, a widget follows each player's light or dark theme. The accent colors the border, stripe, icon, bar, choices and glow.
- **Place:** *Where* puts the widget **On the screen**, **Over the area** (centered over the middle of the trigger's area, like a sign) or **Over the tank that set it off** (just above it, following it, like a name tag, for everyone it shows to). On the screen, **Top**, **Middle**, **Bottom**, **Left**, **Center** and **Right** jump to the usual spots; *Across* and *Down* fine-tune it, and widgets stay inside the screen at any size. A floating widget moves with the arena and draws under the HUD. **Point the way when out of view** keeps it near the edge of the screen, clear of the HUD, with an arrow toward what it floats over, so players can find a vault or chase a marked tank. A widget over a tank hides while that tank is down, and shows nothing when no tank set the trigger off: timers have none.
- **Motion:** appear at once, or **Fade**, **Pop**, **Slide** in from the nearer side or **Drop** from above. **Pulse** gently brightens the border to draw the eye. Widgets fade out when they go. Players with **Reduced motion** on see them appear and go at once.
- **Bar:** none, a set amount, a counter (full at a value you choose) or the time left before the widget goes. **Pieces** splits it into up to 20 segments, for lives or charges. Bars glide to new values.
- **Choices:** each choice sends a signal when a player clicks it, and that player is the tank that set off whatever listens. The menu closes for that player unless **Stay open after a pick** is on. Clicking a choice never fires the tank's gun. **Arrange** sets them in a **Row** (one under another when the row doesn't fit), a **Column** of equal buttons, or a **Grid** two to a row.
- **Who and how long:** show it to **Everyone** (players who join later see it too), the **Players in the area**, or the **Player that set it off**. *Hide after* takes it away after that many seconds; 0 keeps it until a Hide UI trigger or the round ends.

**Hide UI** takes a slot's widget away, or every map widget with slot 0, from everyone, from the players in its area or from the player that set it off. Bots never see widgets.

The bundled [Arcade Hall pack](../mods/arcade_hall) is a finished map built this way: a coin counter with a jackpot, a shop, a vault countdown and a vote. Enable it on Mods to play it, or import its `mod.json` in the workshop to see each trigger.

For a shop, put an On enter trigger at the counter that shows a menu to the player that set it off, and On signal triggers for each choice whose *Who* is *Tank that set it off*: Repair tanks, Award points, Fling tanks, or a counter for a door. Large menus shrink to fit small windows. The logic preview shows widgets over the canvas, and you can click their choices there as the test tank.

### The UI palette

| Piece | What it does |
| --- | --- |
| Message | Shows a card to everyone for 8 seconds as the round starts |
| Counter display | Keeps counter 1 at the top left as a five-piece bar |
| Choice menu | Asks the player who drives in to pick: Repair sends signal 1, Points sends signal 2, Leave closes it |
| Sign | Floats a glowing sign over its area for the whole round |
| Tank marker | Marks the tank that drives in for everyone, with an arrow while it's off screen |
| Hide UI | On signal 1, takes every map widget away |

### Boards

A board is a grid of up to 32 by 32 squares holding up to 128 cells: a control panel, a keypad or the front of a computer. The [boards tutorial](boards-tutorial.md) builds a switch panel that opens a vault. Turn on **Show a board of cells** in a Show UI trigger and the board takes the place of the widget's text, bar and choices. The designer appears under it:

- **Columns** and **Rows** size the grid, and **Square size** and **Gap** set how big it draws. Shrinking the grid keeps every cell inside it.
- **Click adds** picks what a click on an empty square adds. Click a cell to pick it, drag it to move it, and drag the small square in its corner to make it wider or taller. Where cells overlap, the later one draws on top and takes the click.
- **Copy cell** puts a copy of the picked cell in the first free place; a numbered button counts on and takes the next free channel. **Remove cell** and **Clear all** take cells away.
- **Add number keys** lays out keys 0 to 9 in the usual three columns, each on its own free channel: key 0 signals the first one and each key after it the next. **Add 8 bit lights** adds a row of lights for bits 8 down to 1 of counter 1.

The picked cell's settings follow the grid: its kind, words, place and size, and what makes it light up.

| Cell | What it is |
| --- | --- |
| Label | words on the panel |
| Button | sends its channel's signal when clicked, with the player who clicked as the tank that set it off |
| Screen | dark glass with words on it, for messages and readouts |
| Digits | glowing seven-segment digits, like a calculator or a clock: 0 to 9, a minus sign, dots, colons and the letters segments can draw, such as A, b, C, d, E, F, H, L, P and U |
| Light | a lamp, lit or dark, with an optional caption |
| Meter | a bar filled from a counter, full at the value you choose. A tall meter fills from the bottom up |

Every cell's words fill in [counter fields](#counters), so a Digits cell with `{3|2|1:4}` shows a twelve-digit number and a Screen with `{5:b8}` shows a byte. Buttons and lights can be **Lit by** a counter that isn't 0, one bit of a counter, or a group while it's shown, or stay lit always. **Text size** 0 fits the words to the cell, and **Own color** and **Own background** set a cell apart from the widget's look.

A board stays up when a button is clicked, and each player can click up to 20 times a second. Its looks, place, motion, audience and *Hide after* work as for any widget; take it away with Hide UI. The logic preview shows boards over the canvas, and clicking a button there sends its signal as the test tank.

### Conditions and gates

**Only if** gives a trigger a gate. When the trigger fires, it checks its conditions and acts only if the gate is open. Press **Add a condition** to start one, and **Add another condition** for up to eight. Each condition is a card with its own kind:

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
| Even number | an even number hold, none included | XNOR |
| Odd number | an odd number hold | Parity |
| Most | more than half hold | Majority |

With one condition, *Not all* and *None* both open when it fails, which makes a NOT gate.

A shut gate does nothing by default, and doesn't use up *Once per round*, the rest time or *Chance*. Switch on **Otherwise send a signal** to send on the *Otherwise channel* instead (it starts at the lowest channel nothing uses), which makes an if/else: the trigger acts when the gate is open and signals when it's shut. If all 255 channels are in use, free one before adding an otherwise signal. Otherwise signals count toward the chained actions a tick, so they can't loop forever.

While the preview runs, every card shows **holds** or **fails**, and the gate shows how many hold and whether it is **open** or **shut**, updated live. Gated triggers show a small diamond on their canvas badge, and their tag reads like `AND of 2 → Hide` or `#1≥3 → Signal 4`. The Logic tab's trigger list spells each gate out in full.

### Live gates

A gate on an ordinary trigger is only checked when that trigger fires. A **live gate** follows its inputs by itself. Set a trigger's *When* to **On input change** and its action to **Gate to counter**: its output counter then holds 1 while the gate is open and 0 while it's shut, and updates the moment an input changes.

Live gates wire together like real logic. One gate's output counter is the next gate's input, and a chain settles within the tick. With them, a map can hold adders, latches, registers, comparators and counters, and with enough triggers, a working computer. A gate that feeds itself makes memory: the **Latch** piece turns on with one counter, off with another, and holds in between. A loop that never settles stops at the tick's budget and carries on next tick, so it can't freeze a server.

### Timing

For every trigger:

- *Chance* is the percent chance it acts when it fires, and can make a coin flip.
- *Delay before acting* waits that long after the trigger fires.
- *Once per round* stops it after its first action, until a Reset triggers action rearms it.

Signals, counters and group changes can chain: a Show action can set off an On shown watcher, which counts, which sets off a Goal. Each tick allows 32,768 chained actions and 262,144 checks of triggers that heard something, acting or not, so a loop stops instead of freezing the server. Up to 16,384 delayed actions can wait at once, and a tick carries out at most 4,096 outward effects such as moving groups or dropping shapes. Map changes and new rounds reset the logic.

Triggers keep their listeners in lookup tables built when a round starts, so a signal or counter change only visits the triggers listening for it. A map with all 4,096 triggers costs no more per tick than the triggers that actually fire.

### Links

With **L** on, the canvas draws every connection in the map, between linked triggers and from triggers to the objects they act on. A channel, counter or group with many ends meets at a small hub, so a busy map stays readable and quick to draw, even on slower computers.

- **Violet:** a signal. A thinner violet wire is an otherwise signal.
- **Amber:** a counter one trigger changes and another watches or checks.
- **The group's color:** a group one trigger switches and another watches or checks, or the group's objects.
- **Red:** a Reset triggers action and the triggers it rearms.

Point at a hub to lift its wires out of the rest and see how many triggers send and listen on it. Click it to select them all; Shift-click adds them to the selection. Selecting or pointing at a trigger lifts its own wires the same way.

The key at the upper right of the canvas counts each kind of link. Click a kind to hide or show it, which the workshop remembers. On a large map, wires heading to one part of the map run together as a cable and fan out when they arrive, and hubs rest as small dots until the pointer comes near.

### Logic preview

Press **P** to run the map's triggers in the editor with the same engine servers use. Timers count, groups switch on the canvas, and triggers flash when they act. The Logic tab shows everything:

- **Play/Pause**, **Step** (one tick, 1/60 s, while paused), **Restart** and a speed of ×0.25 to ×4. Slow motion helps to follow a fast chain.
- How many delayed actions are waiting.
- **Counters:** counters 1 to 8, with any other the map uses or that isn't 0. Click one to add 1, Shift-click to take 1 away and right-click to clear it. Watchers and goals hear the change as if a trigger made it.
- **Groups:** every group the logic uses. Lit groups are shown; click one to show or hide it, as a script would. Watchers and gates see the change.
- **Send:** pick a *Signal channel* and press **Send signal**. If any trigger waits for a boss, **Defeat** pretends the chosen boss was just defeated.
- **What happened:** counters, announcements, widgets, picks, bosses, shapes, repairs, damage, points and script events.
- **Widgets:** Show UI widgets that reach the test tank appear over the canvas. Click a choice or a board's button to pick it.
- **Triggers:** each trigger's summary with live chips: *spent* for a once-per-round trigger that acted, *next 4.2 s* for a timer, *next sends 7* for a sequence, *open* or *shut* for a gate. **Fire** makes a trigger act once right now, starting the preview if needed.

**Hold T** to drive a test tank at the pointer through plates and exits; the Logic tab sets its team and whether it counts as a bot. **Shift+P** restarts. Editing a trigger restarts the preview by itself.

### The Gates palette

The **Gates** tab in Build holds ready-made logic, each piece drawn with its own symbol. Each one starts on counters 1 and 2, signal 1 and group 1; change them in the Object tab, or pick a group in the dock first. Hover a tile to see what it does.

| Piece | What it does |
| --- | --- |
| Gate | Passes signal 1 on as signal 2 while counter 1 is at least 1 |
| AND, OR, XOR | Pass signal 1 on as signal 3 while counters 1 and 2 are both on, either is on, or just one is on |
| NOT | Passes signal 1 on as signal 3 while counter 1 is off |
| NAND, NOR, XNOR | Pass signal 1 on as signal 3 unless both are on, while both are off, or while both match |
| Majority | Passes signal 1 on as signal 3 while two or more of counters 1, 2 and 4 are on |
| Parity | Passes signal 1 on as signal 3 while one or all three of counters 1, 2 and 4 are on |
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

### The Circuits palette

The **Circuits** tab holds [live gates](#live-gates) and the parts of a computer. Live pieces, marked with a green lamp on their tile, keep their output counter up to date with no signal.

| Piece | What it does |
| --- | --- |
| Live AND, OR, NOT, XOR | Keep an output counter at 1 while the gate is open, else 0 |
| Sum bit, Carry bit | Keep counters 4 and 5 at the sum and carry of counters 1, 2 and 3. Together they make a full adder |
| Latch | Counter 3 turns on with counter 1, off with counter 2, and holds between |
| Compare | Keeps counter 3 at 1 while counter 1 is at least counter 2 |
| Register | On signal 1, copies counter 1 into counter 2, which keeps it until the next |
| Flip bit | On signal 1, flips counter 1 between 0 and 1 |
| Number | Sets counter 1 to 10 as the round starts |
| Pulse | Sends signal 1 ten times a second to step a circuit along |

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
- triggers signal each other in a loop with no delay, gate or chance, which the arena cuts short every tick;
- a Shove body acts on a group that isn't pushable, or a Play path plays a path with no points;
- a timer or At start trigger reaches the tank that set it off, which a timer never has;
- a widget that stays until hidden shows `{left}` or a time-left bar, which only work with *Hide after*.

Notes never block a playtest; scripts can send signals and change counters and groups the checks can't see.

## Workshop plugins

Packs can add tools and checks to the workshop. Enable a plugin pack on **Mods**, review any findings, then open the **Tools** tab. Each tool shows its settings and a Run button. A run changes the map as one undo step and usually selects what it made; **Ctrl+R** runs the last tool again with the same settings. Plugin checks add amber notes to the Check tab and never block a playtest or export.

The bundled **Workshop Toolkit** adds Repeat, Ring of blocks, Arena frame, Snap to grid and Four-way mirror, and a check for gaps too narrow to drive through. The bundled **Logic Kit** builds logic for you: **Switch door** turns the selected blocks into a door with switches that open it when all or any are pressed, **Lever bank** wires two to four levers to an AND, OR or XOR gate that opens the selected blocks, **Countdown** counts a free counter down from the start of the round and announces a message at zero, and **Sequence** makes the selected triggers act one after another when a tank enters a new plate. Its **Unread counters** check notes counters that change but that nothing reads. Plugin packs only change the editor, so enabling one never marks a match as modded. After editing a plugin's scripts, press **Reload plugins**. A tool's result passes the same checks as an import before the workshop keeps it, and a tool that errors or runs too long changes nothing. The full plugin guide covers pack creation, installation, tools, parameters, checks, helpers and debugging. [Editor plugin guide](editor-plugins.md)

## Particle generator

Place an emitter from **Decor**. Its Object tab has ten presets (Embers, Rain, Dust, Sparks, Fireflies, Smoke, Snow, Glitter, Fountain and Pulse) and every setting: shape, where particles start, rate, lifetime, speed, direction, spread, gravity, drag, start and end size, spin, wobble, twinkle, fade, glow and both colors. Direction uses screen coordinates: 0° right, 90° down.

- **Shape:** dot, square, spark, ring, triangle, soft (a dot with a soft edge, for smoke, mist and glows) or star.
- **Emit from:** anywhere in the emitter (Fill), inside the ellipse it encloses (Round), along its outline (Edge) or from its center.
- **Wobble** sways each particle sideways as it travels, for smoke and fireflies. **Twinkle** makes each one flicker.
- **Glow** adds particles' light to what lies beneath them, like embers or sparks. In a dark arena, the area around the emitter is lit as well.

Particles are drawn from seeds, so they cost the server nothing and send no network traffic. A map has at most 48 emitters, each with at most 240 live particles, and the client draws at most 1,024 map particles, scaled by the Effects setting. Reduced motion hides map particles and freezes spin, pulse and conveyor marks.

## Rules

The Rules tab sets movement, acceleration, coasting, damage, health, regeneration, experience, score and respawn time for the playtest and the exported pack. Standard, Arcade, Endurance and Drift are starting profiles. These are map-pack rules, not permissions or server security settings.

An authorized host can change the same rules during a match with **Server → Rules**. Health changes keep each tank's health percentage, and players keep their upgrades and scores. Customized matches do not earn normal arena records.

## Files and deployment

Closed gates can divide an arena into rooms as long as the map's logic can open them. Hosting such maps requires **1.5.2 or later on both the server and players**; earlier versions can validate the project but reject the map data when joining.

The workshop folder is `workshop/` inside Tank Yard's data directory. **Files → Open folder** reveals it. It contains:

- `draft.json`: the recovery draft.
- `projects/<map-id>.json`: named editable documents.
- `exports/<map-id>-<timestamp>.zip`: portable exports.
- `prefs.json`: panel sizes, canvas switches, navigation settings, the palette tab and item, the inspector tab, the logic preview's speed, and where you were looking in the map you had open.

Each export contains `mod.json` and `editor.json`. Gameplay uses the manifest, and the editor document lets others keep editing. The ZIP holds no compiled code. Maps that use conveyors, portals, signals, counters, filters or shapes need protocol 25 (Tank Yard 0.15) on both server and clients. The trigger features added in 0.15.1 (While inside, On empty, On destroy, Random signal, Set counter, Calculate, Script event, single-condition gates and the tank actions) need 0.15.1 to host a map or play it offline. The logic added in 0.16.0 (On change, On shown, On hidden, On boss down, Sequence, Reset triggers, the Tanks inside and Round time conditions, several conditions in one gate and otherwise signals) needs 0.16.0 to host a map or play it offline; older servers refuse such a map with a clear error. Tank Yard 1.0 adds motions, light flicker and the new particle settings, and maps that use them need 1.0. Tank Yard 1.1.0 adds launch pads, Tint, Scale, Animate, Stop, Shake screen and Play sound; it uses protocol 28, so 1.1.0 and 1.2.0 servers and players play together, and maps that use the new pieces need 1.1.0 to open. Tank Yard 1.3.0 adds pushable bodies, paths, ice, gravity wells, darkness and lighting, decor that responds to tanks, the new decor shapes, group names and the Physics triggers; it uses protocol 29, so 1.3.0 servers and players play only with each other, and maps that use the new pieces need 1.3.0 to open. Tank Yard 1.4.0 adds Show UI, Hide UI and the *Tank that set it off* filter; it uses protocol 30, so 1.4.0 servers and players play only with each other, and maps that use the new pieces need 1.4.0 to open. Tank Yard 1.5.0 adds widgets that float over an area or a tank, choice arrangements, glow and the new icons; it uses protocol 31, so 1.5.0, 1.5.1 and 1.5.2 servers and players play only with each other, and maps that use the new pieces need 1.5.0 to open. Tank Yard 1.6.0 adds widget shapes, borders, fills, lettering, title colors, bar looks, button looks and the one-click looks; it uses protocol 32, so 1.6.0 servers and players play only with each other, and maps that use the new looks need 1.6.0 to open. Tank Yard 1.7.0 adds boards, 255 counters and signal channels, larger counters, the bit and power math, counters moved along by other counters, the new counter formats and the new Zoom view settings, and allows 4,096 triggers and 4,096 decor items a map; it uses protocol 33, so 1.7.0 servers and players play only with each other, and maps that use any of these need 1.7.0 to open. Tank Yard 2.0.0 adds the Even, Odd and Most gates, up to eight conditions, On input change, Gate to counter and timers as short as 0.05 seconds; it uses protocol 34, so 2.0.0 servers and players play only with each other, and maps that use any of these need 2.0.0 to open. Maps from earlier versions open unchanged.

For a dedicated server, put the ZIP in persistent storage, add its path to the top-level `mods` list, and run `reload-mods`. Then `maps` lists it and `map my_arena` selects it. Joining clients receive the checked map data automatically. See [Hosting](hosting.md) and [domain setup](hosting-domain.md).

Export and playtest check dimensions, ids, text, bounds, object budgets, trigger settings, portal exits, spawn and base clearance, objectives and starting connectivity. A failed export leaves the document editable and shows the reason in Check. The schema is documented in [Modding](modding.md#maps).
