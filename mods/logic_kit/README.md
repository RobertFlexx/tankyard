# Logic Kit

Example Arena Workshop plugins that build map logic out of triggers, written
in Luau against the editor plugin API described in
[Modding](../../docs/modding.md#workshop-plugins).

Enable the pack on the Mods page, then open the map editor's **Tools** tab:

- **Switch door** turns the selected blocks into a door and places switches
  that open it, when all of them have been pressed or when any one is.
- **Countdown** counts a free counter down once a second from the start of the
  round, shows it on a label and announces a message at zero.
- **Sequence** makes the selected triggers act one after another, a set time
  apart, when a tank enters a new plate.

It also adds a **Signal loops** note to the Check tab when triggers set each
other off with no delay, which the arena cuts short every tick.

The pack only adds editor tools, so enabling it never changes a match.
