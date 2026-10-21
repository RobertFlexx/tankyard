# Playing Tank Yard

Play or Enter joins the main server at tankyard.kokonico.me. Play offline is beside it; Practice is just below. F6 opens Practice from the menu or during a game. Open Offline modes to choose the local arena, bot count, and difficulty. Beginner gives you more time to react; Expert has faster decisions and more accurate aim. Every difficulty uses the same combat stats and upgrade rules as a player.

**Parties** hosts an invite-only match on the leader's computer. Internet invites detect a public IP and try temporary router forwarding; the setup screen explains any manual step still needed. Click the tank icons at the side for members, local voice mutes and party chat. Returning to the main menu keeps the party connected; Resume party returns to play. Leave party explicitly to disconnect. See [party setup](parties.md).

Join server and Settings → Servers have separate Favorites and Recent tabs. Click Sort to cycle through saved/recency order, A–Z, and Z–A. Use × to remove an entry, the arrows to reorder favorites in saved order, or the star beside a recent entry to save it. Page controls reach every saved server. Each list can be cleared independently; changing its display order preserves connection history. Changes and sort preferences save immediately.

On your first launch, choose Dark or Light from two live arena previews. The choice is saved; F9 or Settings changes it later. A small bar below your tank fills as your next manual shot reloads. Auto-firing and drone-only tanks hide the reload bar. Custom mods may still add charge weapons.

Reset to defaults appears beside Save settings on every Settings tab. It applies and saves the default preferences and controls, keeping your name, progress, achievements, practice records, saved servers, mods and signing keys. VSync changes still require restarting the game.

Map cover has distinct artwork, including Garden planters and Circuit Maze walls. Cover follows the animated arena border. The adaptive arena reaches its configured full size at sixteen participants; arrivals get faster expansion while departures retain gradual shrinkage.

## Controls

| Action | Default control |
| --- | --- |
| Move | WASD |
| Aim / fire | Mouse / left click |
| Auto-fire | E |
| Ability | Space |
| Recall drones | Right click |
| Microphone mute / unmute | M or the bottom-right microphone button |
| Previous / next spectator target | Left / Right or spectator buttons, while dead |
| Open text chat | Enter while alive, or Slash, on community servers |
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

Settings → Display applies window sizes immediately; Save settings reapplies the selected size. In fullscreen, resolution selects rendering detail while the desktop stays on the current monitor. Native uses that display’s pixels. F11 preserves the current monitor and restores the selected window size when leaving fullscreen.

Interface scale changes the actual menu and HUD size from 80% to 140%. Drag to preview the percentage; release to apply. The layout stays fixed while dragging, and the control stays in view after applying. Enlarged menus scroll with the mouse wheel or right-hand scrollbar; navigation and Save settings stay fixed. Colorblind palette changes both FFA and all four team colors and adds distinct team symbols. High-contrast labels use bright ink on solid dark backing in menus and gameplay. Accessibility includes a live preview of these changes. Reduced motion stops decorative spins, particles, menu animation, floating score text, camera easing, shake, and arrival flashes; combat movement and attack warnings remain visible.

Effects intensity controls particles and impact flashes, and zero disables them. Shape quality changes curve detail. Frame limit and FPS display apply immediately; VSync and window antialiasing require a restart, as their labels indicate. Supported antialiasing also applies to fullscreen rendering at a selected resolution. Texture filtering affects mod textures; supported levels can be selected and saved before loading a texture pack. Unsupported hardware levels remain disabled. Built-in vector tanks do not change. Audio offers master, proximity voice, and boss-alarm volume, plus an alarm preview. Muted or unavailable audio reports why the preview cannot play.

F1 shows your current bindings. Controls can be rebound in Settings, with a button to restore their defaults. Esc cancels a pending rebind; Back and Esc return to the previous menu. Settings save when you leave their screen. F1, F6, and F10 also close their screen when pressed again. Chat input and Sandbox controls take priority over these shortcuts.

You spawn protected for 15 seconds while stationary. Looking around keeps protection active; movement ends it immediately. A subtle white pulse and the HUD countdown show when you are protected. Weapons and abilities wait until you move or protection expires, including when auto-fire is saved.

Riposte upgrades from Destroyer at level 30. Press Space just before a shot hits to parry it. The window lasts 0.18 seconds, with a 4.2-second cooldown. A successful parry boosts returned damage and heals 8% of your maximum health. Counterguard unlocks at 45 with a 0.16-second window, a 4-second cooldown, a stronger return, and 12% healing. Reprisal unlocks at 55 with a 0.14-second window, a 3.8-second cooldown, two extra return shots, and 15% healing. Their ordinary cannons improve too: Riposte deals 23 damage every 0.44 seconds, Counterguard 27 every 0.36 seconds, and Reprisal 32 every 0.31 seconds, before skill modifiers. A timed parry against a Smasher launches it away and removes half its full health bar, with normal kill credit. Returned shots home toward the shooter and count as your damage and kills. Mines, traps, and other placed defenses cannot be parried.

Parry drones or Factory minions to capture them into a single large drone. Additional captures merge into it, increasing its size, contact damage, and durability up to a tier-dependent cap. Hold left click to steer it toward your cursor; let go to chase the original attacker, even while auto-fire is on. Right click recalls it. If the attacker is gone, it seeks nearby enemies. Riposte's capture lasts six seconds, Counterguard's seven, and Reprisal's eight; it expires when you die. Re-parrying cannot renew a returned drone's remaining lifetime. Captured minions lose their cannons.

Drone tanks guide their drones toward the cursor while firing. Recall them before moving through a dangerous lane. Factory is a bit different: its six minions have actual cannons, settle around your cursor when commanded, and shoot nearby enemies or shapes on their own when you let go. The Factory keeps making replacements until it has six again, never more. Right-click still recalls the group, same as Tank Yard’s other drone classes. Watch where you park them: walls block their bullets, and they need room to get a clean shot.

Twin, Gunner, and other alternating guns use slightly independent firing phases: they stagger, drift, and sometimes fire together. Multi-cannon bursts keep their short three-shot spacing while their longer reload phases drift. Their average timing follows the configured reload rate. Client recoil prediction uses the same timing rules as the server.

Friendly drones press into a compact swarm at the cursor. They collide and slide around each other, with room for each drone's outline. They also slide along walls and arena edges. Right-click recall keeps the swarm around your tank; redirecting it restores cruising speed. Small movements inside a resting swarm keep their previous visual heading.

Homing rounds make gradual corrections toward targets ahead. A close sidestep can beat their turn rate, and a shot that passes you cannot turn back. They expire after at most 2.2 seconds or 850 units of travel; automatic homing turrets have a shorter 1.8-second lifetime. Riposte returns follow the same limits.

In a text field, use arrows or Home/End to place the caret. Hold Shift to select, double click a word to select it, or drag across text. Ctrl+A selects all; Ctrl+C/X/V copies, cuts, and pastes. Password fields allow paste but keep their text out of the clipboard. Backspace and Delete repeat when held. Ctrl with an arrow, Backspace, or Delete works by word.

## Progression

Levels 15, 30, and 45 need 1,020, 8,644, and 35,304 XP. The final upgrade tier unlocks at level 55, with 72,734 XP. Each level-45 class has a route into the new tier. Skills arrive at three points per four levels: 10 points at level 15, 21 at level 30, and 33 at level 45. After level 45, you earn one point every two levels, ending at 38 points at level 55. Each skill still caps at seven points. Sandbox level controls use the same point budget.

Trapper reloads in one second and its traps deal 27.6 damage with 5.49 penetration. Every trap launcher has 20% shorter reloads, 20% more damage, 22% more penetration and 30% faster launch speed compared with v0.2.15. Traps launch farther and settle more gradually. The Trap Range skill increases launch distance, up to about 2.15 times the base range at seven points. Mixed weapons show Speed / Trap Range in the same skill slot. Trap lifetimes and the 24-trap limit stay the same. Automatic mounts only fire at targets their shots can reach. Auto Smasher bullets last 2.2 seconds.

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

Shoot shapes to earn points before fighting other tanks. Tank upgrades unlock at levels 15, 30, 45, and 55. For bullet builds, spend points on damage, penetration, and reload. Find cover when your health is low. In Skirmish, spend your starting points before the first fight.

## Bots and multiplayer

Bots farm, evolve, use abilities, and play objectives. Their neural models learn visible movement and human class results, as well as their own fights and skill choices. They remember opponents who hurt or killed them: cautious bots keep more distance, while bolder bots may try to settle a grudge when healthy. Winning eases that tension, and grudges cool down over time. Memory survives respawns and resets each match. Reaction delays, aim error, and normal combat stats still apply.

Sight ranges are 420 / 500 / 560 / 620 units from Beginner through Expert, including automatic mounts. Walls block targeting and threat detection. Bots sidestep incoming shots and pull back from nearby drones or approaching homing rounds. Shots already fired keep their normal flight range. Smasher bots close in, approach heavy cannons from the side, and retreat when hurt. Parry bots wait until a visible shot is about to hit.

Online arenas fill to six tanks by default: 1–5 players get 5–1 bots, and 6 or more get none. Bots return as players leave. Co-op wave enemies are counted separately. Empty servers pause the arena until someone joins. Offline games keep your chosen bot count.

Spawn protection lasts up to 15 seconds while stationary. Protected tanks pulse white; Reduced motion uses a steady tint. Aiming keeps protection; moving ends it. Weapons and abilities are inactive until then. Reconnect on the disconnect screen retries the same server.

When you die, the camera follows a living tank. Use Left/Right or the Previous/Next buttons to switch. Team modes and Co-op PvE only allow watching teammates. The view automatically changes when the watched tank dies or leaves, and returns to your own tank after respawning. If nobody can be watched, the death screen stays open. Survival elimination and Co-op wave revives keep their normal rules.

Proximity voice works online within 900 arena units and fades with distance. The microphone starts muted each connection and round. Press M or click the bottom-right microphone icon to toggle it; the icon lights green while sending speech. Muting the microphone closes its capture stream and still lets you hear nearby players. Settings → Audio has master and voice volume sliders. Capture and playback use your operating system's default devices. Use headphones to avoid feedback. Missing devices show an error on the microphone control without interrupting gameplay; click it again to retry. Voice stops during menus and while dead, and server mutes also apply to voice. `--no-audio` disables capture and playback entirely.

On community servers, Enter opens chat while alive and Slash opens it even while spectating. Escape closes the chatbox; the last five wrapped lines stay readable on a shaded background. Chat moves to the right while upgrades or Sandbox tools are open. Scroll through older messages while the chatbox is open. Enter remains the respawn key outside the chatbox.

## Achievements

Per match is the default. Each achievement can be claimed once in a round, including across deaths and respawns; starting another game or a new round resets claims. Global keeps claims and kill totals across matches on this device. Off disables claims and popups without deleting saved global records. Select the mode on the Achievements page. The ten-round achievement is available in Global. Sandbox, Practice, and modded games do not count.

Choose Host server to play with friends or Join server to enter an address. A local match pauses when you open its menu; a shared server keeps running. The [hosting guide](hosting.md) covers LAN and internet connections, moderation, and match controls.

## New combat trials and Chronarch

Practice (F6) now has Foundations and Combat trials pages. Counter school teaches body parries, Frozen front introduces the Chronarch, Sentinel hunt is a one-life boss duel, and Hold the line tests trap placement. Each challenge saves its own personal best and supports Retry. Offline modes also offer a bot-count slider, level 1/30/45 starts, arena-size presets, and an adaptive-border toggle. Preset levels disable arena achievements.

Chronarch is a level-55 evolution from Counterguard or Relay. Its Stasis wave ability has an 18-second cooldown: for three seconds it catches your own and hostile kinetic rounds within 330 units, up to 96 rounds. They move with the tank without aging, then release radially as your shots. Rounds cannot be captured repeatedly. Friendly teammates' rounds, drones, traps, mines, and melee bypass the field; cover blocks it. Killing the caster cancels conversion and restores the original rounds.

The verified creator has a gold **Game Creator** badge. Sharing an arena with another verified human creator unlocks **Meet the game's creator** according to your achievement preference, including modified arenas. A matching display name or server moderation role cannot grant the badge.

[Bastion Anticheat](bastion-anticheat.md) validates player intentions while the server runs movement, weapons, damage, and upgrades. Delayed or reordered inputs do not trigger cheating accusations or automatic bans. Signed creator powers are authorized changes to those server rules.

Server hosts can open moderation with the Server button or F8. Creator and central-owner menus require the matching private key; see [server administration](creator-administration.md). Warnings and announcements appear in a centered banner and can be dismissed.

Owner tools also have a bottom-right toggle. If the server cannot authorize the central-owner identity, clicking it explains the missing setup. Creator tools → Maps applies a map from the server's catalog, preserving tank progress and clearing transient shots. A layout that cannot safely fit current tanks, bosses or objectives is rejected.

## Parties, maps and the tank tree

[Parties](parties.md) let friends join a private match on the leader's computer with a random IP invite. Play co-op waves, Boss Rush or other offline modes together; allied bots fill empty team slots. Internet play requires a reachable host and UDP port forwarding. Invite access never grants the leader's tools.

Dedicated servers rotate to an appropriate map every two hours of process uptime, even while empty. The bottom-left timer continues through round changes. Hosts can set `map_rotation_seconds = 0` to disable rotation; private parties disable it by default.

The Tank tree shows every upgrade connection. Scroll over the tree to zoom around the pointer. Drag empty space, or use the middle/right mouse button, to pan. Click a tank for its model, strengths and tradeoffs. Fit all restores the overview; Focus centers and enlarges the selected tank. Background gameplay on the main menu pauses with Reduced motion.

## Infection and King of the Hill

Infection starts with an eight-second preparation period once at least two tanks are present. One random Patient Zero becomes infected. Survivors win by lasting up to three minutes; infected win by converting everyone. Death after the outbreak converts survivors and automatically respawns them, preserving their build. Late arrivals join the infected. Your role and the remaining time appear at the top. Parties keep a fixed total population instead of recreating converted survivors.

King of the Hill has one capture point that relocates every 45 seconds. Capture it, stay inside to score, and contest it to stop the other team. The first team to the score target wins. Bots defend and retarget the hill. Both modes work offline, in parties and on dedicated servers (`--mode infection` or `--mode king-of-the-hill`).

All text boxes support Ctrl/Cmd+A/C/X/V and Shift+Insert for paste. Clipboard work runs separately from rendering. Cutting only removes a selection after copying succeeds; a pending paste is discarded if focus or editing changes. Party copy confirmation appears only after a successful native clipboard write.
