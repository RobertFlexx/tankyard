# Animate and effects

Build a small effects room: a lantern that floats and sways, an arrow that grows when you approach, a chime and flash when it finishes, a switch that freezes the lantern, and a launch pad.

You need **Tank Yard 1.1.0 or later**. The [playable examples on the site](https://robertflexx.github.io/tankyard-site/#effects-tutorial) show the same combinations. The bundled [Effects Lab pack](../mods/effects_lab) contains a finished map you can play or edit.

## Open the finished example

Enable **Effects Lab** on Mods, then choose **Effects Lab** in an offline Sandbox game. Or open **Map editor → Files → Import pack** and choose its pack folder or ZIP. Press **P** to preview, **F5** to play, and Escape to return to the editor.

To build it yourself, start with a new open map. Leave the spawn in clear floor. The example uses a 2400 × 1600 arena; the effects work at other sizes too.

## 1. Put the lantern in a group

1. Choose group **1** in the dock, then **Build [1] → Decor → Lantern** and place its light in the left half of the room.
2. Add a **Tile** from Decor at the light's centre, still in group 1. Make it a small upright rectangle and give it a warm color. This is the lantern's body; its shape makes Sway visible, while the circular light alone would look unchanged when rotated.
3. If you placed either part earlier, select the light and tile together and put them in group 1 in Object. Keep surrounding blocks and pads in group **none**.
4. Press **G** to show group numbers. Check that both lantern parts say 1. Effects target a group; they do not need to sit on top of the decor they control.

The lantern is decoration. A tank can drive beneath it; its animation will not move collision.

## 2. Make it float

1. In **Effects**, place **Float** near the lantern.
2. Press **Edit [2]**, select Float, and open **Object**.
3. Set **Target group** to **1**. Its action is **Animate decor**, its style is **Bob**, and it starts at the beginning of the round.
4. Use **Size 14**, **Times a second 0.6**, and **Over 1 second**. Keep **Curve: Sine**, **Mode: In and out** and no trip back.
5. Press **P**. Over the first second, the lantern eases into a gentle vertical movement.

Size is the animation's amplitude, not its total distance from top to bottom. Bob at size 14 travels 14 world units above and below its placed position. A pace of 0.6 means a cycle takes about 1.67 seconds.

Changing **Times a second** keeps the current phase. It changes the pace without snapping the lantern to a different point in its cycle.

## 3. Combine two styles

Place **Sway** from Effects and target group **1** again. Keep its preset **Size 12**, **Times a second 0.5** and **Over 1 second**. Restart the preview with **Shift+P**.

The lantern now floats and rocks at the same time. Each style has its own channel:

- **Bob:** up and down, in world units.
- **Sway:** back and forth, in degrees.
- **Throb:** grows and shrinks, in percent.
- **Shake:** irregular trembling, in world units.
- **Orbit:** circles its placed position, in world units.

Another Bob replaces the group's current Bob. A Sway plays beside it. To settle one style, send Animate decor for that style with **Size 0** and a short Over; the other styles keep playing.

## 4. Grow an arrow when a tank enters

1. Place an **Arrow** from Decor in the right half of the room, in group **2**.
2. Place **Grow** from Effects on the floor in front of it. Stretch its rectangle into an area a tank can cross.
3. Set **Target group: 2**, **When: On enter**, **Size: 140%**, **Over: 0.4 seconds**, **Curve: Back**, **Mode: Ease out**, and **Go back after: 2 seconds**. Leave its one-second rest time.
4. In the preview, hold **T** with the pointer in the Grow area. The arrow enlarges with a small overshoot, rests, then returns to its normal size.

Scale changes appearance, not collision. Put a block next to the arrow to see the difference: the block stays the same size even if it shares the group.

## 5. Sound and flash on arrival

1. Place **Chime** from Effects near the arrow. Change **When** to **On arrive** and **Watch group** to **2**. Use **Volume 80%** and a short rest time, such as **0.5 seconds**.
2. Place a **Neon** from Decor beside the arrow, in group **3**.
3. Place **Flash** from Effects. Set **When: On arrive**, **Watch group: 2** and **Target group: 3**.
4. Keep Flash's white tint at **90%**, **Over 0.08 seconds**, **Quad / Ease out**, **Go back after 0.12 seconds**, and a one-second rest time.
5. Restart the preview and enter Grow again. The chime and white flash happen when the arrow finishes growing. Its return counts as another arrival too.

The watcher and the target are different: group 2 completes the motion; group 3 receives the flash. This avoids making a flashing group repeatedly react to its own tint finishing.

Sound comes from the Chime trigger's position. It fades with distance and follows the player's Effects volume. It is not a global soundtrack.

## 6. Freeze the lantern

1. Place **Sender** from Logic on another patch of floor. Leave it sending on channel **1** when a tank enters.
2. Place **Stop** from Effects. Keep **When: On signal**, **Listen on channel: 1**, and **Target group: 1**.
3. Run the preview and move the test tank into Sender. The lantern stops exactly where it is, including its bob and sway.
4. Restart with **Shift+P** to run the room from the beginning again.

Stop freezes all the group's active channels, including movement, rotation, fades, tints and scale. It also cancels scheduled trips back. A stopped effect does not count as arriving. Use **Size 0** instead when you want one style to settle gently while other effects continue.

## 7. Add a launch and a small tremor

1. From **Pads**, place a **Launch pad** on open floor. In Object set **Direction 315°** and **Strength 1**. Leave clear space up and to the right for its landing.
2. Place **Quake** from Effects over the Grow area. Use **Strength 30%**, **For 0.6 seconds**, and a **3-second** rest time.
3. Press **F5**. Cross Grow to feel the tremor, then drive over the launch pad. It flings you along the arrows and gives you a short coast before steering takes over.

Launch pads affect movement and collision. Decor effects do not. A launch is predicted using the server's movement rules, so it remains responsive online.

## Check and share

- Open **Check** and resolve errors before exporting.
- Test at normal speed and with **Reduced motion** enabled. Animations and shake should stop; tint and scale remain visible. Keep useful cues in color or shape as well as movement.
- Check sounds nearby and from the far side of the room. Avoid stacking loud alarms on a short timer.
- Save the editable project, then Export a pack. Server and players need 1.1.0 or later for these effects.

The finished [Effects Lab](../mods/effects_lab/mod.json) uses ordinary JSON map data. It has no script and changes no tank stats or rules. The separate [Arena Effects script pack](../mods/arena_fx) shows how Luau can play sounds, shake views and control effects in response to round and map events.

## Script and plugin authors

The same effect can be started from a gameplay script:

```lua
tankyard.on("round_start", function()
  tankyard.animate_map_group(1, "bob", 14, 0.6, 1)
  tankyard.animate_map_group(1, "sway", 12, 0.5, 1)
end)
```

Scripts run in the simulation; they do not play client code. Read [the effect APIs](modding.md#changing-the-match) for arguments and limits. Workshop plugins place the same trigger fields through the [editor API](editor-plugins.md); they do not need their own animation engine.
