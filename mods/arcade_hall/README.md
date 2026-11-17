# Arcade Hall

Requires Tank Yard 1.5 or later.

A neon hall that shows what the 1.5 workshop can do with UI triggers. Every widget is plain map data, so open it in the workshop to see how it's built.

- **Welcome card:** as the round starts, every player gets a card with their own name in it.
- **Coin counter** (slot 2): a pill on the right shows counter 1 out of 20 as ten pieces. Drive over the four gold coins on the **prize floor** to add to it. Every coin chimes.
- **Jackpot:** at 20 coins, a banner with a draining timer bar announces the jackpot, the lights flash, 24 shapes rain on the prize floor and the counter starts over.
- **Shop** (slot 6): the teal counter opens a six-choice grid for you. Repair, take 300 points, get a vault waypoint, read the rules, raise the stage lights for eight seconds, or leave. Player rewards use the *Tank that set it off* filter. Driving away closes the menu.
- **Vault** (slot 3): a countdown pill runs for 40 seconds, then the west vault's gate opens, its lamp turns green and the first tank inside wins 1,000 points with a card of its own. A crown marker follows the winner for eight seconds, using slot 32 and `{cause}` for their name.
- **Call the next event** (slot 7): every 50 seconds, anyone can pick **Lights out** or **Shape rain**. The first pick wins and closes the menu for everyone.

Import the folder on Mods, enable it and choose Arcade Hall in Offline modes, or start a server with `--mod mods/arcade_hall --map arcade_hall`. The workshop can import its `mod.json` to edit it.
