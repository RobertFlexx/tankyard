# Logic gates tutorial

This tutorial builds a small vault puzzle in the Arena Workshop, one piece at a time. It covers what's new in 0.16: watchers, gates with several conditions, otherwise signals, sequences, resets and the live Logic tab. Allow about fifteen minutes. The [workshop guide](map-editor.md#groups-and-logic) is the full reference; this page is the hands-on version.

The finished puzzle:

- Two levers on the floor, each with a label showing whether it's on.
- A vault wall that opens while **both** levers are on, and closes again when either is turned off.
- An announcement whenever the vault opens, however it opened.
- A reset plate that turns both levers off and closes the vault.
- A warning light that blinks through three colors while the vault is open.

You need Tank Yard 0.16.0 or later.

[Watch the logic gates video](logic-gates-tutorial.mp4) (1 min 42 s) for a tour of the prepared vault, its AND and XOR gates, otherwise signals and live preview. [English captions](logic-gates-tutorial.vtt) are also available. The steps below build the whole circuit, including the lights and reset plate.

## 1. Start a map

1. Open **Map editor** from the main menu and press **New** (twice if it asks).
2. In **Map**, name the arena `Vault`. The two spawn regions are already placed; keep them clear.
3. Press **L** so the canvas draws logic links, and **G** so it shows group numbers.

## 2. Build the vault wall

1. Press **1** for Build and pick **Blocks**.
2. In the dock, press **Free** next to Group. The dock now reads group 1, and every block you draw goes in it.
3. Drag out a wall of three or four blocks across the middle of the arena. A group-1 badge sits on each block.
4. Set the dock's group back to **none** (the − button) so the next pieces don't join the wall.

Everything in group 1 shows and hides together. Hiding the group opens the vault.

## 3. Place the levers

1. Open the **Gates** tab in the dock. Hover any tile to read what it does.
2. Place a **Lever** to the left of the wall. Each time a tank enters it, counter 1 flips between 0 and 1.
3. Place a second **Lever** on the right. Press **2** for Edit, click it, and in the Object tab set *Counter* to **2**.
4. From **Decor**, add a **Label** above each lever. Set their text to `Left {1}` and `Right {2}`. In a match, `{1}` and `{2}` show the counters' values.

Press **P** to start the preview, hold **T** and drive the test tank across a lever. Open the inspector's **Logic** tab: the counter changes as you cross. You can also click a counter there to add 1, Shift-click to take 1 away and right-click to clear it, so you can test logic without driving. Press **Shift+P** to restart when you're done.

## 4. Hear the levers move

The vault needs to check the levers whenever either one moves. An **On change** watcher fires every time its counter's value changes, whatever changed it.

1. From **Logic**, place a **Receiver** near the left lever. In Object, set *When* to **On change** and *Watch counter* to **1**. Set *Do* to **Signal**, and *Send on channel* to **1**.
2. Place another the same way for counter **2**, also sending signal **1**.

With links on, an amber wire runs from each lever to its watcher, and a violet wire from each watcher to anything listening on signal 1. Nothing listens yet, so the Check tab would note that signal 1 is unheard.

## 5. Build the AND gate

1. From **Gates**, place an **AND** between the watchers and the wall. It listens on signal 1 and already has two conditions: counter 1 ≥ 1 and counter 2 ≥ 1, combined with **All (AND)**.
2. In Object, change *Do* from Signal to **Hide**, and set *Target group* to **1**, the wall.
3. Scroll down to **Only if**. Each condition is a card. Leave both as they are.

Now the gate hides the wall when signal 1 arrives, but only if both levers are on.

Run the preview and click counter 1, then counter 2 in the Logic tab. The wall disappears after the second click. While the preview runs, the AND's condition cards show **holds** or **fails**, and the gate shows **2 of 2 hold** with an **open** pill.

## 6. Close it again with an otherwise signal

The wall stays open even after a lever goes back off, because a shut gate does nothing by default. Fix that with an otherwise signal:

1. Select the AND gate. Under the conditions, switch on **Otherwise send a signal**. *Otherwise channel* starts at the lowest channel nothing uses yet, which is **2** here; check it reads 2.
2. From **Logic**, place a **Receiver** listening on signal **2** with the action **Show** on group **1**.

The gate is now an if/else: when signal 1 arrives, it hides the wall if both levers are on, and otherwise sends signal 2, which shows the wall again. On the canvas, a thin violet wire marks the otherwise signal.

Test it: click counter 1 and counter 2 to open the vault, then right-click counter 1 to clear it. The wall comes back.

### Try the other gates

Select the gate and change **Combine**:

- **Any (OR):** opens while either lever is on.
- **Just one (XOR):** opens while exactly one lever is on; turning on the second closes it.
- **Not all (NAND)** and **None (NOR):** the opposites of AND and OR. With None, the vault is open until someone pulls a lever.

**Add another condition** adds up to four. Try a third card with *Round time* ≥ 60, so the vault can't open in the first minute, or *Tanks inside* ≥ 2 on a plate that two players must stand on together.

## 7. Announce the opening

An **On hidden** watcher fires whenever its group is hidden, by a trigger, a timer or a script.

1. From **Gates**, place an **Alarm**. It already watches group 1 and announces `The door is open!`. Change the text to `The vault is open!`.

Because the Alarm watches the group itself rather than the gate, it still works if you later add a second way to open the vault.

## 8. A warning light that cycles

A **Sequence** sends on the next channel of a run each time it acts.

1. From **Decor**, place three **Lights** around the vault, each in its own group: choose **Free** in the dock before placing each one, then set the dock back to none. Give them three colors.
2. From **Gates**, place a **Sequence**. It sends signals 1, 2 and 3 in turn, which the levers already use, so set *First channel* to **10** in Object. Its three channels are now 10, 11 and 12. Change *Every (s)* to **0.5**.
3. Add three Receivers on signals 10, 11 and 12, each with **Toggle** on one light's group.
4. Select the Sequence and click **Add a condition**. Make it **Group hidden** with group **1**, so it only runs while the vault is open.

In the preview, the Logic tab's trigger list shows the Sequence's *next sends* chip ticking through 10, 11 and 12, and its gate chip reads open or shut as the vault opens and closes.

## 9. A reset plate

1. Place a **Plate** from Logic near a spawn. In Object, set *Do* to **Signal**, *Send on channel* to **20** and *Rest after acting* to **3**.
2. Add two Receivers on signal 20 with **Set counter**: one sets counter 1 to 0, the other counter 2 to 0. The On change watchers hear both, the gate shuts and sends signal 2, and the vault closes by itself.

If your puzzle uses *Once per round* switches, a **Rearm** from Gates makes a group's triggers start afresh: they can act again, their rest times end and waiting delays are dropped. Put the switches in a group, point the Rearm at it and send its signal from the reset plate.

## 10. Check and play

1. Open the **Check** tab; it checks the map as it opens, and **Check again** repeats it. Fix anything red. Amber notes point out logic that can never work, such as a signal nothing receives or a watcher on a counter nothing changes.
2. In the **Logic** tab, use **Step** (while paused) to follow a chain one tick at a time, or set the speed to ×0.25. The trigger rows show what's spent, waiting and open.
3. Press **F5** to playtest. Drive over the levers and watch the vault.
4. **Save** the project and **Export** a pack when you're happy with it.

## Shortcuts for logic work

| Keys | Action |
| --- | --- |
| P, Shift+P | Run or pause the preview, restart it |
| Hold T | Drive the test tank at the pointer |
| L | Show logic links |
| G | Show group numbers |
| Click, Shift-click, right-click a counter | Add 1, take 1 away, clear it |

The bundled **Logic Kit** plugin pack builds this circuit for you: enable it on **Mods**, select the vault's blocks, and run **Lever bank** from the Tools tab. Its source in `mods/logic_kit/tools.lua` is a worked example of building gates from Luau; see the [editor plugin guide](editor-plugins.md).
