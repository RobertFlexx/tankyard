# Arena Effects

Sounds and screen shake for the big moments of a round. A bell rings as each
round starts, every destroyed tank lands with a thump, and each fifth kill in
a row plays a fanfare and briefly shakes every player's view.

Maps can call on two more effects with a **Script event** trigger. An event
named `alarm` flashes group 1 red, makes it tremble, sounds the alarm at the
trigger and shakes the views of the tanks inside its area. An event named
`calm` settles group 1 again.

Enable it in **Mods**, or run `tankyard-server --mod mods/arena_fx`. It needs
Tank Yard 1.1.0 or later. Players' Camera shake setting and Reduced motion
still apply to every shake.
