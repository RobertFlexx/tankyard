# Menus and waypoints tutorial

Build a shop with six choices, a waypoint to a vault and a marker over the player who opens it.

[Watch the video](ui-tutorial.mp4) (35 seconds), with [English captions](ui-tutorial.vtt). It tours the finished [Arcade Hall example](../mods/arcade_hall): the designer, placement settings and a playtest of the shop and waypoint. You need **Tank Yard 1.5.0 or later**.

To explore the example, enable Arcade Hall on **Mods**, open the workshop and import its map through **Files**. Press **2** to edit. At the shop, **Tab** cycles through overlapping triggers until **Show UI** is selected; its settings are in **Object**.

## Make the shop

1. In **Build → UI**, place **Choice menu** where players should enter the shop. Select it and set **When** to **On enter**. Under **Who and how long**, set **Show to** to **Player that set it off**.
2. Pick a free **UI slot**, such as **6**. Each player has 32 map slots; showing another widget in the same slot replaces it.
3. Set the title to **Shop** and the text to **{name}, pick one**. The text preview uses a sample player; a match fills in the viewer's name.
4. Choose **Panel**, the **Coin** icon and an accent color. **Replay entrance** shows the animation again. Without its own colors, the panel follows the player's theme.
5. Set **Where** to **On the screen**, choose **Bottom**, and adjust its position to leave room for the normal HUD.
6. Add six choices: **Repair**, **+300 points**, **Vault guide**, **How to play**, **Stage lights** and **Leave**. Choose the **Grid** layout. Give the first five choices signal channels **1**, **2**, **9**, **10** and **11**. Leave the last at **0**.
7. Leave **Stay open** off. Choosing any option closes this menu; channel 0 closes it without sending a signal.

Each choice sends its channel with the player who picked it as the cause. Add listeners with **When: Signal** on those channels. Use **Repair tanks** on channel 1 and **Award points: 300** on channel 2, both targeting **Tank that set it off**. Arcade Hall shows the complete wiring.

## Point toward the vault

1. Place a **Sign** from **UI** at the vault. Set **When** to **Signal**, channel **9**, and **Show to** to **Player that set it off**.
2. Use slot **30**, title **VAULT**, text **Opens at 0:40**, and the **Lock** icon. Set a **12-second** duration.
3. Set **Where** to **Over the area** and enable **Point the way when out of view**. The sign floats at its area; outside the view, an arrow points toward it from clear of the normal HUD.
4. If the vault moves, give this trigger the same **member group** as its blocks. The sign follows the group's paths, motions and pushed bodies. Hiding the group hides its sign too.
5. To keep the text true once the vault opens, give the sign **Only if: Round time < 40**. Duplicate it with **Ctrl+D**, drag the copy back over the vault, and give the copy the text **Gate open**, the **Key** icon and **Round time ≥ 40**. Both use slot 30, so a newer pick replaces the older sign.

The shop's **Vault guide** choice now shows a personal waypoint. Another player's choice does not put it on your screen.

## Mark the winner

Place **Tank marker** from **UI** on the vault's entrance. Use **Show to: Everyone**, slot **32**, **Where: Over the tank that set it off**, the **Crown** icon and an eight-second duration. A title such as **{cause} opened the vault** names the tank that triggered it. **{name}** is always the person viewing the widget; **{cause}** is the person who set it off.

Use a signal instead of **On enter** if opening the vault already has a trigger. The signal carries its cause through delays and gates. A tank marker needs a tank to follow; it stays hidden when there is none, the tank leaves, or it is out of the viewer's arena.

## Check and play

Open **Check**, then point at the shop on the canvas and press **Shift+F5**. Pick **Vault guide** and check the arrow. Press **Escape** to return, start another playtest and pick **+300 points**. The score rises without firing your tank's weapon through the menu.

**Save** keeps an editable project; **Export** makes a pack for a server or the Mods page. Update the server and players to 1.5.0 before using the new widgets. The [UI reference](map-editor.md#ui) covers every setting, and [HUD widgets in Luau](modding.md#hud-widgets) covers scripted menus and pins. To restyle the shop, follow the [widget styles tutorial](widget-styles-tutorial.md).
