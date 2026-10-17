# Playing Tank Yard

Play or Enter joins the main server at tankyard.kokonico.me. Play offline is beside it; Practice is just below. F6 opens Practice from the menu or during a game. Open Offline modes to choose the local arena, bot count, and difficulty. Beginner gives you more time to react; Expert has faster decisions and more accurate aim. Every difficulty uses the same combat stats and upgrade rules as a player.

## Controls

| Action | Default control |
| --- | --- |
| Move | WASD |
| Aim / fire | Mouse / left click |
| Auto-fire | E |
| Ability | Space |
| Recall drones | Right click |
| Spend a skill point | 1 through 8, or click the skill |
| Fill a skill | Hold Shift while pressing its key or clicking |
| Tank upgrades | Tab |
| Pause / menu | Esc |
| Respawn | Enter or the Respawn button |
| Light / dark theme | F9 |
| Fullscreen | F11 |
| Controls and shortcuts | F1, or Help in the HUD |
| FPS and network details | F3 |
| Restart an offline game from pause | F5 |
| Practice | F6, then 1 through 4 to start a challenge |
| Settings | F10 |
| Sandbox level up / down | PgUp / PgDn; hold Shift for 15 levels |
| Next / previous text field | Tab / Shift+Tab |

F1 shows your current bindings. Controls can be rebound in Settings, with a button to restore their defaults. Esc cancels a pending rebind; Back and Esc return to the previous menu. Settings save when you leave their screen. F1, F6, and F10 also close their screen when pressed again. Chat input and Sandbox controls take priority over these shortcuts.

You spawn protected for up to 15 seconds. Moving, firing, or using an ability ends that protection immediately. The HUD shows the time left.

Riposte upgrades from Destroyer at level 30. Press Space just before a shot hits to parry it. The window lasts 0.45 seconds, with a 4.5-second cooldown. Returned shots home toward the shooter and count as your damage and kills. Drones, mines, and other placed weapons cannot be parried.

Drone tanks guide their drones toward the cursor while firing. Recall them before moving through a dangerous lane. Factory is a bit different: its six minions have actual cannons, circle your cursor when commanded, and shoot nearby enemies or shapes on their own when you let go. The Factory keeps making replacements until it has six again, never more. Right-click still recalls the group, same as Tank Yard’s other drone classes. Watch where you park them: walls block their bullets, and they need room to get a clean shot.

Twin, Gunner, and other alternating guns use slightly independent firing phases: they stagger, drift, and sometimes fire together. Multi-cannon bursts keep their short three-shot spacing while their longer reload phases drift. Their average timing follows the configured reload rate. Client recoil prediction uses the same timing rules as the server.

Friendly drones press into a compact swarm at the cursor. They collide and slide around each other, with room for each drone's outline. They also slide along walls and arena edges. Right-click recall keeps the swarm around your tank; redirecting it restores cruising speed. Small movements inside a resting swarm keep their previous visual heading.

In a text field, use arrows or Home/End to place the caret. Hold Shift to select, double click a word to select it, or drag across text. Ctrl+A selects all; Ctrl+C/X/V copies, cuts, and pastes. Password fields allow paste but keep their text out of the clipboard. Backspace and Delete repeat when held. Ctrl with an arrow, Backspace, or Delete works by word.

## Modes

Random Events plays like Free For All, with a 20-second event followed by an eight-second break. Overdrive speeds up movement and reloads; Double points doubles XP and score; Regeneration restores health; Shape rush adds bonus shapes.

Co-op PvE has ten waves of raiders, with bosses on waves 3, 6, 9, and 10. Everyone starts on the same team at level 15. The bot setting adds friendly teammates. Clear a wave to heal the squad and revive fallen players with their builds intact. A squad wipe ends the match. Dead players follow a living teammate until the next wave.

## Practice

Each challenge uses built-in tanks, a fixed loadout, Normal bots, and normal simulation rules. The timer pauses with the offline game. You have one life; dying ends the attempt. Completed and unfinished attempts are saved locally, with your best result and completion time. Practice does not award normal arena achievements. Use R or F5 to retry, or Esc to return to the challenge list after a result. Starting another game saves the session you are leaving.

You can also launch a challenge directly, for example `./tankyard --practice drone_drill`.

| Challenge | Goal | Starting tank |
| --- | --- | --- |
| Shape sprint | Earn 500 score within 90 seconds | Level 1 Basic, no enemy tanks |
| Drone drill | Earn 1,500 score within two minutes | Level 45 Overlord with skills assigned, no enemy tanks |
| Skirmish | Destroy three tanks within three minutes | Level 30 Twin against four bots on Mirrored Circuit |
| Last stand | Survive two minutes or become the last living tank | Level 1 Basic against seven bots in Closing Ring |

Shoot shapes to earn points before fighting other tanks. Tank upgrades unlock at levels 15, 30, and 45. For bullet builds, spend points on damage, penetration, and reload. Find cover when your health is low. In Skirmish, spend your starting points before the first fight.

## Bots and multiplayer

Bots farm, spend points, evolve, use abilities, retreat, and pursue mode objectives. A small neural network learns motion trends from successive visible positions. It adjusts a bounded part of their aim prediction; reaction delays and aim error still apply. Reversals reduce confidence. Damage and deaths make them more cautious, while successful fights reduce caution. Memory survives respawns and resets with the match. Bots use the same combat stats as players.

Online arenas fill to six tanks by default: 1–5 players get 5–1 bots, and 6 or more get none. Bots return as players leave. Co-op wave enemies are counted separately. Empty servers pause the arena until someone joins. Offline games keep your chosen bot count.

Spawn protection lasts up to 15 seconds while idle. Protected tanks pulse white; Reduced motion uses a steady tint. Moving, firing, or using an ability ends protection. Reconnect on the disconnect screen retries the same server.

## Achievements

Per match is the default. Each achievement can be claimed once in a round, including across deaths and respawns; starting another game or a new round resets claims. Global keeps claims and kill totals across matches on this device. Off disables claims and popups without deleting saved global records. Select the mode on the Achievements page. The ten-round achievement is available in Global. Sandbox, Practice, and modded games do not count.

Choose Host server to play with friends or Join server to enter an address. A local match pauses when you open its menu; a shared server keeps running. The [hosting guide](hosting.md) covers LAN and internet connections, moderation, and match controls.
