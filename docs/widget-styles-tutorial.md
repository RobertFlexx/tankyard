# Widget styles tutorial

Give a Show UI widget a look of its own: start from a preset, then change single parts. Widgets that scripts show take the same options.

[Watch the video](widget-styles-tutorial.mp4) (38 seconds), with [English captions](widget-styles-tutorial.vtt). You need **Tank Yard 1.6.0 or later**.

## Start from a look

1. In the Arena Workshop, select a **Show UI** trigger and open **Object**. Press **P** to run the logic preview. When the trigger fires, its widget appears over the canvas as players will see it.
2. Under **Look**, click one of the twelve presets: **Theme**, **Neon**, **Arcade**, **Parchment**, **Terminal**, **Royal**, **Glass**, **Hazard**, **Sticker**, **Frost**, **Ember** or **Jungle**. A look sets the colors, shape, border, fill, lettering, bar and buttons. The words, the place and the choices stay as they were.
3. Press **Shift+P** to restart the logic preview with the new look. The inspector's own preview, above **Words**, changes at once.

**Theme** puts the widget back on each player's own light or dark colors.

## Change single parts

- **Shape:** panels and cards can be **Rounded**, **Square**, have **Cut corners**, lean (**Slanted**), end in a point (**Tag**) or have notched ends (**Ribbon**). Pills stay round and banners stay straight.
- **Border:** **Line**, **None**, **Thick**, **Double**, **Dashed** or **Corners**, which draws brackets at the corners.
- **Fill:** **Solid**, **Gradient**, **Glass**, **Stripes**, **Grid** or **Dots**. **Own fill color** picks the second color of a gradient or pattern.
- **Lettering:** **Auto**, **Clean**, **Shadow**, **Block**, **Outline** or **Glow**. **Capital letters** applies to the title, the text and the choices, and **Own title color** colors the title.
- **Bar look:** **Capsule**, **Flat**, **Thin**, **Striped**, **Glow** or **Pips**. Pips show one dot per piece, or 10 when the bar isn't split.
- **Buttons:** once the widget has choices, **Choices** offers **Capsule**, **Box**, **Outline** or **Underline**.

## Play it

Press **F5** to play the map. With **Reduced motion** on, striped bars and fills stay still. Maps that use these options need Tank Yard 1.6.0 on the server and for every player.

## In a script

`tankyard.hud` takes the same options:

```lua
tankyard.hud("vault", {
  title = "Vault", text = "The vault is open", style = "card", icon = "key",
  shape = "slant", border = "thick", fill = "stripes", fill_color = "#ffc800",
  lettering = "outline", caps = true, panel = "#181818", accent = "#ffc800",
  progress = 0.6, bar_style = "striped", buttons = "box", choices = { "Enter", "Wait" },
})
```

[HUD widgets](modding.md#hud-widgets) lists every field, and the [UI reference](map-editor.md#ui) covers the rest of the designer.
