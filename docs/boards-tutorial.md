# Boards tutorial

Build a control panel: four switches on a board, a readout that shows them as binary, and a vault that opens when they spell the code. You need **Tank Yard 1.7.0 or later**.

## The vault

1. In the Arena Workshop, place a wall from **Blocks** across a corner of the arena.
2. In **Object**, set its **Group** to **1**. This is the vault door.

## The board

1. From the **UI** palette, place a **Message**. In **Object**, set *Hide after* to **0** so it stays all round, and put it at the right of the screen with **Right** under *Place*.
2. Under **Board**, switch on **Show a board of cells**. The board takes the place of the message's text, and the designer appears under it.
3. Set **Columns** to **4** and **Rows** to **2**.
4. Under **Click adds**, pick **Digits**, then click the top-left square of the grid. Drag the small square in the cell's corner to the right edge, so the cell covers the whole top row. In the cell's **Text**, type `{1:b4}`: counter 1 in binary, four digits long.
5. Under **Click adds**, pick **Button**, then click each square of the bottom row from left to right. Each new button takes the lowest signal channel nothing uses. On a new map that's 1, 2, 3 and 4; if yours differ, use the numbers under **Signals channel**.
6. Click each button in turn and give it a **Text** of **8**, **4**, **2** and **1**, the value of its switch. Set **Lit by** to **One bit of a counter**, with **Counter** 1 and **Bit** 4, 3, 2 and 1, so each button lights while its switch is on.

## The logic

1. From **Logic**, place a **Receiver**. Set *Listen on channel* to **1**, *Do* to **Calculate**, the counter to **1**, the math to **Bits XOR** and the number to **8**. Each click flips that switch's bit.
2. Place three more the same way: channel **2** with **4**, channel **3** with **2**, and channel **4** with **1**.
3. Place one more Receiver. Set *When* to **On change**, *Watch counter* to **1** and *Do* to **Hide**, group **1**. Under **Only if**, press **Add a condition** and make it **Counter 1 = 11**. 11 is 1011 in binary: switches 8, 2 and 1 on, 4 off. On counter wouldn't do here: it also fires when the counter jumps past 11, such as from 8 to 12.

## Play it

Press **P** to run the logic preview. The board appears over the canvas: click its buttons and watch the lights and the readout follow. Clicking 8, 2 and 1 hides the vault door. Press **F5** to play the map for real.

To go further, a **Screen** cell can show words with counter fields, such as `Code {1}`, and a **Meter** fills from a counter. **Add number keys** lays out a whole keypad at once, each key on its own channel. The [boards reference](map-editor.md#boards) covers every cell and setting, and [counters](map-editor.md#counters) lists the counter formats and the rest of the bit math.

## In a script

`tankyard.hud` shows boards too. A click on a button raises the pack's `choice` event with the button's place in `cells`:

```lua
local switches = 0
local function panel()
  local cells = { { kind = "digits", span = { 4, 1 }, text = string.format("%d", switches) } }
  for place, value in ipairs({ 8, 4, 2, 1 }) do
    cells[#cells + 1] = {
      kind = "button", at = { place - 1, 1 }, text = tostring(value),
      lit = bit32.band(switches, value) ~= 0,
    }
  end
  tankyard.hud("panel", { title = "Vault", style = "panel", x = 0.85, board = { columns = 4, rows = 2, cells = cells } })
end
tankyard.on("round_start", function() switches = 0; panel() end)
tankyard.on("choice", function(event)
  if event.name ~= "panel" or event.value < 2 then return end
  switches = bit32.bxor(switches, ({ 8, 4, 2, 1 })[event.value - 1])
  panel()
end)
```

[HUD widgets](modding.md#boards) lists every board and cell field.
