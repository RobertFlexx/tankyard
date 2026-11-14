# Motions tutorial

This tutorial builds a small gatehouse in the Arena Workshop, one piece at a time. It covers what's new in 1.0: motions and easing curves, chained motions, moving decor, lights and particles. Allow about fifteen minutes. The [workshop guide](map-editor.md#motions) is the full reference; this page is the hands-on version. To get a feel for the curves first, the [Tank Yard website](https://robertflexx.github.io/tankyard-site/#motions) plays every curve and mode in your browser.

The finished gatehouse:

- A door that slides open as a tank drives up, and shuts four seconds after the last one arrives, shoving anyone in the doorway clear.
- A wall that glides back and forth across the yard as moving cover.
- Three barricades that land one after another as the round starts, then an announcement.
- An arrow sign that turns, and a warning light that blinks each time the door comes to rest.
- Flickering torches with rising embers, a fountain and fireflies.

You need Tank Yard 1.0 or later.

## 1. Start a map

1. Open **Map editor** from the main menu and press **New** (twice if it asks).
2. In **Map**, name the arena `Gatehouse`. The two spawn regions are already placed; keep them clear.
3. Press **G** so the canvas shows group numbers, and **L** so it draws logic links.
4. Press **F9** for the dark theme. Lights glow strongest on a dark floor; press **F9** again whenever you want to see the map in the light theme.

## 2. Build a door

1. Press **1** for Build and pick **Blocks**. Draw a wall down the middle of the arena with a gap three cells high halfway along. Leave room to drive around both ends of the wall: the map check needs every part of the arena to be reachable while the door is shut.
2. In the dock, press **Free** next to Group. The dock now reads group 1. Fill the gap with blocks: this is the door, and a group-1 badge sits on each block.
3. Open the **Motion** tab in the dock. Hover any tile to read what it does.
4. Drag out a **Door** over the gap, reaching about a tank's length out on both sides of the wall. A tank opens the door by driving into this dashed area. Because group 1 is chosen in the dock, the Door moves group 1 rather than joining it.
5. Set the dock's group back to **none** (the − button).

Press **2** for Edit and click the Door. In Object:

- *When* is **On enter**: a tank crossing into the area sets it off. *Rest after acting* is 1 second.
- *Do* is **Move group to**, with *Target group* 1. *Below its place* is **−6**, so the door slides to 6% of the arena's height above where the map puts it: up into the wall, clear of the gap.
- *Over* is 0.5 seconds, along the **Cubic** curve with **Ease out**: the door starts fast and settles gently. The graph draws the curve, and a dot rides along it to show the feel.
- *Go back after* is 4: four seconds after it opens, the door slides shut. Each tank that arrives while it's open starts the four seconds over.

With the Door selected, the canvas draws a ghost of the door where the motion takes it, joined by an arrow. The ghost should sit over the wall, clear of the gap; if any of it still covers the gap, drag the diamond at the arrow's head higher. The diamond snaps to the grid, and Shift keeps it straight.

**Move group** slides a group a distance from wherever it is, so each tank would push the door a step further. **Move group to** always sends it to the same place, however busy the doorway gets.

## 3. Try the curves

Every motion has a length, a curve and a mode. With the Door selected:

1. Change *Curve* to **Bounce**. The door now bounces as it reaches the top.
2. Try **Back**, which overshoots a little and settles, and **Elastic**, which springs past the end and wobbles back.
3. Change the mode. **Ease in** starts slowly and ends at full speed, **Ease out** starts fast and ends slowly, **In and out** is slow at both ends, and **Out and in** is fast at both ends and slow in the middle.
4. Set it back to **Cubic** and **Ease out**.

| Curve | How it moves | Try it for |
| --- | --- | --- |
| Linear | At one speed from start to end | Conveyors, sliding targets |
| Sine | The gentlest ease | Floating platforms, slow patrols |
| Quad, Cubic, Quart, Quint | Each eases harder than the last | Doors and lifts |
| Expo | Nearly still, then very fast, or the reverse | Pistons, slamming gates |
| Circ | Like Expo, with a rounder turn | Heavy gates |
| Back | Pulls back first or overshoots a little | Signs, hinged barriers |
| Elastic | Springs past the end and wobbles | Bumpers |
| Bounce | Bounces against the end | Things that drop into place |

*Over* sets the length; 0 moves at once. A move may average at most one arena width a second, so dragging the diamond further lengthens *Over* as needed.

## 4. Test the door

1. Press **P** to run the logic preview, then hold **T** to drive the test tank into the Door's area. The door slides up, and a dashed outline marks where the map puts it.
2. Drive away. Four seconds later the door slides home.
3. In the inspector's **Logic** tab, press the Door's **Fire** button to play its motion without driving.
4. Press **F5** to playtest. Stop in the doorway as the door shuts: it shoves your tank clear rather than trapping it. Moving cover pushes tanks, shapes and bosses out of its way. Press **Escape** to return to the editor.

## 5. Add moving cover

1. Pick **Blocks**, press **Free** in the dock (group 2), and draw a short wall in an open part of the yard.
2. From **Motion**, place a **Shuttle** anywhere. It's a timer, so where it sits doesn't matter, and it moves group 2, the group chosen in the dock.
3. Set the dock's group back to **none**.

The Shuttle is **Every** 6 seconds and **Move group**: 15% of the arena's width to the right over 2 seconds, with **Sine** and **In and out**, and *Go back after* 1. Every six seconds the wall glides across, rests a second and glides back. Select the Shuttle and drag the diamond to choose where the wall goes. Bots find new routes as cover moves.

## 6. Land the barricades in turn

**On arrive** acts when a group's motion ends, so one motion can start the next.

1. Draw three short barricades a little above where they should end up, each in its own group: press **Free** before drawing each one, so they take groups 3, 4 and 5. Set the dock's group back to **none**.
2. From **Motion**, place a **Drop** and set its *Target group* to **3**. As the round starts, it drops group 3 by 8% of the arena's height with a bounce. Drag the diamond to choose where it lands.
3. Place a second **Drop**. Set *Target group* to **4**, *When* to **On arrive** and *Watch group* to **3**. Group 4 drops the moment group 3 lands.
4. Place a third: *Target group* **5**, *When* **On arrive**, *Watch group* **4**.
5. Place a **Then** from Motion and set its *Watch group* to **5**. Change *Do* to **Announce** and type `The gates are set!`.

Press **Shift+P** to restart the preview and watch them land in turn. The Logic tab's **What happened** list shows the announcement.

On arrive also acts when a group gets back from a trip, so a door with *Go back after* sets off its watchers twice: once open and once shut. A motion cut short by another doesn't count, and neither does one with nowhere to go, such as a door told to open while it's already open.

## 7. Light the yard

1. From **Decor**, place a **Torch** on each side of the door and a **Lantern** in the middle of the yard. A Torch is a warm light that wavers like a flame, a Neon is a cool, steady glow, and a Lantern is a wide, soft light.
2. Select a torch. *Flicker* sets how much it wavers, from 0 (steady) to 1 (a failing lamp). *Opacity* sets how strongly it shines, and its size sets how far the light reaches: drag a handle to change it.
3. Press **F9** to compare the themes. Lights brighten whatever lies under them, so they glow strongly on a dark floor and softly on a light one. Players with Reduced motion on see lights hold steady.
4. With one torch selected, press **Ctrl+Shift+A**. Every light in the map is selected (torches, neons and lanterns alike), so one change to *Flicker* or *Opacity* reaches them all. Press **Esc** to deselect.

## 8. Make decor move

**Turn decor** and **Fade decor** animate a group's decor and particle emitters. They never change collision.

1. Press **Free** in the dock (group 6) and place an **Arrow** from Decor beside the door.
2. With group 6 still chosen, place a **Spinner** from Motion. Every 3 seconds it turns group 6's decor a quarter turn along **Back**, **Ease out**, so the arrow overshoots a little before it settles. Set the dock's group back to **none**.
3. Place a **Neon** on the other side of the door. Press **2**, click it and press **Ctrl+G** to put it in a new group, group 7.
4. Place a **Blink** from Motion and set its *Target group* to **7**. Change its *When* to **On arrive** and *Watch group* to **1**. Now the light blinks off and back each time the door comes to rest, open or shut.

## 9. Add particles

1. From **Decor**, drag out an **Embers** emitter just above each torch. Embers drift up, sway and glow.
2. Select an emitter. The buttons under **Particles** in Object are presets; each sets every field below it at once. Try **Fireflies**, **Smoke** and **Snow**, then go back to **Embers**.
3. Change *Shape*. **Soft** is a dot with a soft edge, for smoke, mist and glows. **Star** suits glitter, and **Spark** streaks along its direction, like rain.
4. *Emit from* chooses where particles start: anywhere in the emitter (**Fill**), inside its ellipse (**Round**), along its outline (**Edge**) or at its **Center**.
5. *Wobble* sways particles sideways as they travel, and *Twinkle* makes each one flicker. **Glow (adds light)** adds their light to what lies beneath them, as lights do. *Direction* is in screen degrees: 0° points right and 90° down.
6. Place a **Fountain** in the middle of the yard and a **Fireflies** emitter in a dark corner.

Particles are drawn from seeds on each player's computer, so they cost the server nothing and send no network traffic. Players with Reduced motion on don't see map particles.

## 10. Check and play

1. Open the **Check** tab and fix anything red.
2. Press **P** and watch the yard: the shuttle glides, the barricades land in turn, the arrow turns and the torches flicker. Objects away from home show a dashed outline where the map places them.
3. Press **F5** to playtest. Drive through the door, then stand in the shuttle's path.
4. **Save** the project and **Export** a pack when you're happy with it. Maps saved from 1.0 need Tank Yard 1.0 or later to open and play.

The workshop remembers how you left it, down to the palette tab and item, the inspector tab, the preview's speed and where you were looking.

## Shortcuts for motion work

| Keys | Action |
| --- | --- |
| P, Shift+P | Run or pause the preview, restart it |
| Hold T | Drive the test tank at the pointer |
| Shift while dragging the diamond | Keep the motion straight |
| G, L | Show group numbers, logic links |
| Ctrl+G | Put the selection in a new group |
| Ctrl+Shift+A | Select everything like the selection |
| F9 | Switch between the light and dark theme |
| F5 | Playtest |

## Going further

- **Lift** and **Home** move a group to a set place on signals 1 and 2. **Then** sends a signal each time a group finishes a motion, out or back.
- **Piston** punches a group out and pulls it back every 2 seconds. **Fader** fades a group's decor while tanks pass and brings it back after 5 seconds.
- Scripts play motions with `move_map_group`, `turn_map_group` and `fade_map_group`, and JSON map packs use the `move`, `place`, `turn` and `fade` actions. See [Modding](modding.md#motions).
