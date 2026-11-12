# Logic Kit

Example Arena Workshop plugins that build map logic out of triggers, written
in Luau. The [editor plugin guide](../../docs/editor-plugins.md) explains
the API they use.

Enable the pack on the Mods page, then open the map editor's **Tools** tab:

- **Switch door** turns the selected blocks into a door and places switches
  that open it, when all of them have been pressed or when any one is.
- **Countdown** counts a free counter down once a second from the start of the
  round, shows it on a label and announces a message at zero.
- **Sequence** makes the selected triggers act one after another, a set time
  apart, when a tank enters a new plate.
- **Lever bank** places two to four levers and a gate that opens the selected
  blocks while all, any or just one of the levers is on. It shows the gate
  features from 0.16: On change watchers, a gate of several conditions and an
  otherwise signal that closes the door again.

It also adds an **Unread counters** note to the Check tab when a counter
changes but no watcher, condition, label or announcement uses it. The
workshop's own checks already cover signal loops.

The Lever bank needs Tank Yard 0.16.0 or later; the other tools work from
0.15.1.

The pack only adds editor tools, so enabling it never changes a match.
