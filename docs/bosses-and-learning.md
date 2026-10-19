# Bosses, arenas, voice, and learning

Random boss events run in Free For All, Two Teams, Four Teams, Maze, and Random Events. After a 60-second initial grace period, the server makes **one aggregate 0.25% roll per simulation second**. A successful roll chooses uniformly among all eleven boss types. This is not a per-frame or per-type probability. Only one random boss can be active, with a 90-second grace period after it disappears. Dedicated Boss Rush and Co-op waves keep their own schedules; Sandbox bosses remain manual.

Boss arrivals use a reliable, deduplicated event: an original 48 kHz alarm, a brief red pulse, and a top-center banner that expires after five seconds. Reduced Motion suppresses the red flash. Audio Disabled and the master/effects volume controls apply to the alarm independently of microphone mute and voice volume. Bosses remain stationary and cannot deal contact damage during their first two seconds. Spawns search for free space away from tanks and fail safely if the map cannot accommodate them.

| Boss | Silhouette and weapons | Hull radius |
| --- | --- | --- |
| Guardian of the Pentagons | Pink triangle, rear spawner, pursuing triangular swarm | 102 |
| Summoner | Yellow square, four spawners, square drone swarm | 96 |
| Defender | Red triangle, three rotating trap launchers and three auto turrets | 106 |
| Fallen Overlord | Gray circular hull, four flared spawners, faster durable drones | 88 |
| Fallen Booster | Gray Booster hull, front cannon and four rear thrusters, fast pursuit | 82 |
| Sentinel | Hexagonal armor, phased forward shell spreads | 90 |
| Prism | Triangular armor, rotating ricocheting volleys | 90 |
| Foundry | Pentagon, capped drone escorts and deployed turrets | 100 |
| Tempest | Octagonal armor, homing salvos and telegraphed lunges | 78 |
| Bastion | Square armor, rotating mine ring and explosive cannon | 100 |
| Eclipse | Seven-sided armor, phase-dependent radial bursts and charge | 90 |

The classic bosses were researched through the community's original game observations: [Guardian](https://diepio.fandom.com/wiki/Guardian), [Summoner](https://diepio.fandom.com/wiki/Summoner), [Defender](https://diepio.fandom.com/wiki/Defender), [Fallen Overlord](https://diepio.fandom.com/wiki/Fallen_Overlord), and [Fallen Booster](https://diepio.fandom.com/wiki/Fallen_Booster). Their recognizable silhouettes and arsenals are recreated with original vector geometry. Tank Yard uses its own sizes, damage values, phases, spawn schedule, and combat rules. No external game artwork or code is included.

Bosses lead moving targets, prefer recent attackers, maintain target continuity, observe line of sight, avoid approaching projectiles, manage distance while replenishing swarms, steer around walls, and respect the current arena border. Drone populations and projectile queues are capped. Custom charge attacks show a directional warning before accelerating.

Open arena modes animate their border around a fixed coordinate system. Both humans and bots count, including participants waiting to respawn, so deaths do not pulse the map size. Two or fewer participants use 65% of the configured width/height; the border smoothly grows to the full configured dimensions by 24 participants. Smaller configured maps retain at least 900 × 700 playable units. Each border edge moves at most 24 units per second with damped easing; the client also interpolates between network updates. Shape counts follow playable area. Team homes/objectives move with the border, while fixed walls stay in place. Hull constraints resolve a shrinking border pinching a tank against a wall. Maze, Survival, Sandbox, Boss Rush, and Co-op retain their designed layouts or elimination boundaries. Server owners can set `world.dynamic_arena = false`.

Voice is 48 kHz mono Opus at a target 96 kbps, in bounded 20 ms packets, with full-band encoding, in-band FEC, waveform loss concealment, a short jitter buffer, and smoothed proximity gains. Codec work runs outside the audio callback lock. This replaces 16 kHz G.711 companding. Frame validation, per-sender rate limits, server-authoritative distance, mutes, living-player restrictions, and sequence replay rejection remain in force. The codec is statically bundled; no separate Opus install is required.

The Display tab's Native option follows the monitor's reported desktop mode. Monitor queries run periodically, and display changes update the requested resolution. Linux/Windows select the display with the greatest window overlap, including monitors with negative coordinates; macOS uses its reported primary display. High DPI is accounted for without changing the operating system's display mode. Fixed resolution presets and safe graphics mode remain available. Sandbox includes an integer level slider, tier presets, and a 0.25×–2× offline speed slider with a reset button. Online speed controls are disabled, and level slider requests are coalesced below server rate limits. Sliders retain their drag when the pointer leaves the track. Sandbox and server panels reserve space below arrival alerts; chat and tooltips occupy separate HUD regions.

Bots train real, small neural networks online using bounded replay buffers. Motion observations train aim prediction; visible class outcomes from humans and bots and their own loadout results train tank/build preferences. A contextual network chooses combat spacing, flanking, approach, and ally rally maneuvers from actual score, damage, kills, and survival outcomes. Exploration continues so alternatives are not permanently excluded. Strong melee tanks still commit at contact range, while recovery, objectives, visibility, legal movement, and map constraints take precedence.

Cooperation under shared threats builds personal friendship. Betrayal reduces friendship and increases fear/animosity; grudges cool down and memories are bounded. These relationships influence target preferences and support weighting without granting hidden information. Parry decisions use stable per-projectile mistakes and varied timing: intended missed-attempt rates are 48% / 28% / 18% / 10% for Beginner / Normal / Advanced / Expert, before aim, reaction delay, and physical evasion affect the outcome. Repeated frames cannot reroll the same threat into a guaranteed defense.

Learning survives deaths and network weights carry into subsequent rounds. Player sightings, relationships, and tactical trials reset between rounds; learning is not saved across process restarts. These are experience-trained controllers operating within game rules, not claims of consciousness or unrestricted agency.

Stat upgrades immediately update deployed class drones/minions: speed, damage, remaining durability fraction, and minion reload. Upgrades preserve damage already taken, entity identity, lifetime, and location. Captured parry returns retain their tier-specific caps; already-fired ordinary bullets retain their launch stats.

To inspect actual boss geometry without a GPU:

```sh
cargo run --locked -p tankyard-client --bin tankyard-models -- --bosses --columns 4 --output artifacts/models/bosses.svg
```

## v0.2.16 combat update

Bosses now start at roughly 4,590–6,885 health before higher waves and participant scaling, with larger health reserves for defensive hulls. All eleven bosses add a secondary attack cycle with a 0.9-second white-ring windup. The new attacks complement their primary weapons: drone bosses add suppression fans, defensive bosses place trap rings with gaps, pursuers launch seeking volleys and charges, Sentinel fires an aimed piercing fan, Prism/Eclipse alternate patterned rings, and Foundry combines explosive fans with mine screens. The recognizable broad classic silhouettes now have original reactor cores and armor inlays. All shots retain normal wall collision, finite lifetime, projectile caps, and applicable parry/stasis counterplay. The aggregate random encounter chance remains 0.25% per simulation second.

## v0.3.0 balance

Summoner drones acquire players within 820 units of the drone instead of 1000, stop selecting targets beyond 950 units of the boss, and return to guard once beyond a 1050-unit owner leash. Their damage is 18 instead of 20 and four-drone replenishment takes 0.72 seconds instead of 0.6. Swarm caps remain unchanged. Guardian, Fallen Overlord, and Foundry drones also have archetype-specific leashes and observe cover, spawn protection, and the boss's restraint toward distant players below level 15.

Every boss pauses primary fire during its special-attack warning and leaves at least 0.4 seconds after the secondary volley before its next primary. Fallen Booster's normal speed is 180 instead of 195 and front bullet damage is 19 instead of 21. Prism's primary rings fire every 1.45 seconds instead of 1.3, last 4.2 seconds instead of 5, and ricochet once instead of twice. Bastion mines deal 27 instead of 32 damage and last 6.5 instead of 8 seconds. Tempest fires every 1.05 seconds instead of 0.9, with a fan centered on its aim. The larger health reserves and aggregate encounter probability remain unchanged.
