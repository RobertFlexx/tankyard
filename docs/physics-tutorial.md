# Physics and lighting tutorial

Build a dark yard: a room behind a push door, a crate you shove onto a plate to open a gate, a ferry that rides a path, and a lightning storm.

You need **Tank Yard 1.3.0 or later**. The bundled [Night Works pack](../mods/night_works) is a finished map built the same way; enable it on Mods and pick it in an offline Sandbox game, or import it in the workshop to see how it's put together.

Start with **New** in the workshop. The steps use a 2400 × 1600 arena, but everything works at other sizes. Press **G** to show group numbers on the canvas as you go.

## 1. Turn the lights down

1. Open the **Map** tab and find **Lighting**.
2. Set **Darkness** to **70%** and pick a deep blue in the color picker below it. The canvas shows the darkness at half strength so you can keep building.
3. In **Build [1] → Decor**, place a **Lantern** over the spawn region. Lights cut through the dark; so do repair, hazard, launch and portal pads, every tank and every shot.

Press **F5** to see it at full strength, and **Escape** to come back.

## 2. Build a push door

1. Draw a room with **Blocks**: a back wall and two sides, with the open side facing the spawn.
2. Pick group **1** in the dock's **Group** chooser and draw a thin block across the opening. Set the chooser back to **none**.
3. Press **Edit [2]**, select the door and open **Object**. Turn on **Pushable: group 1 is one body**.
4. Set **Slides** to the direction that goes into the room. For a door across a gap in a bottom wall, choose **Up and down**, set **Reach up** to **15%** and **Reach down** to **0%**, so it only opens inward.
5. Set **Spring back** to **12** so it swings shut when you let go, and **Friction** to **8**.
6. Name the group **Door** in the same tab. Names show in the Map tab's group list and in trigger summaries.

Playtest and drive into the door. It gives way at your speed, stops at the end of its reach, and slides home behind you. It never crushes a tank: if you sit in the doorway, it waits.

## 3. Put a roof on it

1. In **Decor**, place a **Roof** over the whole room. It's a tile drawn above tanks.
2. In **Object**, its **Near a tank** setting is **Fade away**. Raise **Within** if you want it to open sooner.

From outside, the room is dark and covered. Drive in and the roof clears for you, and only for you: other players still see it shut.

## 4. A crate on a plate

1. Pick group **2** and draw a square block in open floor. This is the crate.
2. Select it, turn on **Pushable**, keep **Slides: Any way**, and set **Weight** to **1.5** and **Bounce** to **30%**. Turn on **Shots push it** so you can knock it along by firing.
3. Pick group **3** and draw a block that closes a gap somewhere. This is the gate. Set the chooser back to **none**.
4. In **Build → Physics**, place **Crate plate** where you want the plate, and **Plate release** on exactly the same spot. Draw a green **Tile** under them so players can see the plate.
5. Select **Crate plate**. Set **Watch group's blocks** to **2** and **Target group** to **3**. It acts **On touch**: when any block of the crate starts to overlap the plate, the gate hides.
6. Select **Plate release** and set the same groups. It acts **On release**: when the crate's last block leaves the plate, the gate comes back.
7. For a sound, add **Chime** from Effects on the plate too, with **When: On touch** and **Watch group's blocks: 2**.

Only cover presses these plates. A tank parked on the plate does nothing; the crate has to be there.

## 5. Ride a ferry

1. Pick group **4** and draw a long, thin block: the ferry. Add a **Beam** from Decor on top of it, still in group 4. Decor in a group rides along with its blocks.
2. In the **Map** tab, under **Paths**, press **Add path**. Name it **Ferry** and set **Repeat** to **There and back**.
3. Press **Add point**. Set **Right of its place** to **15%**, **Seconds to get here** to **2.5** and **Wait here** to **0.8**. Choose the **Sine** curve, **In and out**.
4. Add a second point at **15%** right and **12%** below, with **1.5** seconds and the same wait.
5. In **Build → Physics**, place **Path**, then set its **Target group** to **4** and **Path** to **1**.

While the Map tab is open, the canvas draws the path from where the ferry rests through its numbered points, with a dot riding it at its own pace and a ghost of the ferry at the point you're editing. Press **P** to run the preview and watch the ferry go. In a playtest it shoves tanks out of its way.

## 6. Bring a storm

1. In **Build → Physics**, place **Lightning** anywhere. Every 9 seconds it sets the darkness to 0% almost at once, and **Go back after 0.15 s** returns it to what it was.
2. For thunder, place **Chime** from Effects, pick the **Rumble** sound, and set **When: Every**, **Every 9 s** and **First wait 1 s**, so it rolls a second after each flash.
3. For a lamp switch, place a **Lighting** trigger with **When: On enter** at a spot tanks drive over. Set **Darkness 15%**, **Over 1.5 s** and **Go back after 10 s**. The yard brightens while that lamp is on, then sinks back.

Lighting changes reach every player at once. Players with **Reduced motion** on see sudden changes, such as the flash, ease in over about half a second instead.

## 7. Throw some tanks

1. Place **Fling pad** in open floor. Select it and set **Direction** and **Speed**: 0° is right and 90° down. Tanks that enter fly that way and coast for a moment.
2. Place **Blast** in the middle of the yard. Every 4 seconds it throws tanks outward from its middle. A negative speed pulls them in instead; the **Vortex** piece does that twice a second.
3. Place **Zoom out** at the room's door, so the view widens for 6 seconds when a tank drives through.

## Check and play

Open **Check**. Amber notes point out unfinished pieces, such as a Shove body on a group that isn't pushable or a Play path with no points, and never stop a playtest. Click a note to find the trigger.

Press **F5** to play, then **Export** to make a ZIP you can share. The map needs Tank Yard 1.3.0 on the server and every player.
