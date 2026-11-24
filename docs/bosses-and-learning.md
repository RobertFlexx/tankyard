# Bosses, arenas, voice, and learning

Random boss events run in Free For All, Two Teams, Four Teams, Maze, and Random Events. After a 60-second initial grace period, the server makes **one aggregate 0.25% roll per simulation second**. A successful roll chooses uniformly among the fourteen regular bosses. About one arrival in eighty is a Railgunner instead, as long as the arena is worth at least two full-level tanks. This is not a per-frame or per-type probability. Only one random boss can be active, with a 90-second grace period after it disappears. Dedicated Boss Rush and Co-op waves keep their own schedules; Sandbox bosses remain manual.

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
| Foundry | Armored workshop hull, capped drone escorts and deployed turrets | 100 |
| Tempest | Arrowhead hull, homing salvos and telegraphed lunges | 78 |
| Bastion | Square armor, rotating mine ring and a heavy cannon that hits for 125 | 100 |
| Eclipse | Dark disc in a spiked ring, phased radial bursts | 90 |
| Siege Engine | Tracked chassis and a huge cannon whose shot destroys any tank it reaches | 104 |
| Skirmisher | Twin cannons, fast orbit and a straight charge | 72 |
| Trapper | Twin trap launchers and side-pocket mines | 88 |
| Railgunner | Huge armored hull, a barrel that splits open for its plasma beam, hunts the furthest player | 132 |

Every boss is drawn in one style: an outlined hull, darker armor plates, grey guns where its rounds leave, and one bright core that whitens as a special attack charges. Tank Yard's bosses use their own sizes, damage values, phases, spawn schedule and combat rules.

Bosses weigh distance, recent damage, movement, nearby allies and the targets other bosses are chasing. Each species favors different situations and varies its choices without constantly switching targets. They lead moving targets, observe line of sight, dodge approaching projectiles, manage distance while replenishing swarms, steer around walls, and respect the current arena border. Drone populations and projectile queues are capped. Custom charge attacks show a directional warning before accelerating.

Open arena modes animate their border around a fixed coordinate system. Both humans and bots count, including participants waiting to respawn, so deaths do not pulse the map size. Two or fewer participants use 65% of the configured width/height; the border smoothly grows to the full configured dimensions by 24 participants. Smaller configured maps retain at least 900 × 700 playable units. Each border edge moves at most 24 units per second with damped easing; the client also interpolates between network updates. Shape counts follow playable area. Team homes/objectives move with the border, while fixed walls stay in place. Hull constraints resolve a shrinking border pinching a tank against a wall. Maze, Survival, Sandbox, Boss Rush, and Co-op retain their designed layouts or elimination boundaries. Server owners can set `world.dynamic_arena = false`.

Voice is 48 kHz mono Opus at a target 48 kbps, in bounded 20 ms packets, with full-band encoding, in-band FEC, waveform loss concealment and smoothed proximity gains. The microphone passes a 75 Hz high-pass filter before the speech gate, so a DC offset or desk rumble cannot hold it open, and the quiet frame before speech is sent too, so first syllables are not clipped. Each speaker plays from a jitter buffer that starts at 40 ms and grows a frame at a time, up to 120 ms, when packets arrive late, then shrinks after ten calm seconds; playback fades in and lets go instead of clicking, and a mix of loud voices is soft-limited rather than clipped. Codec work runs outside the audio callback lock. Frame validation, per-sender rate limits, server-authoritative distance, mutes, living-player restrictions, and sequence replay rejection apply to every packet. The codec is statically bundled; no separate Opus install is required.

The Display tab's Native option follows the monitor's reported desktop mode. Monitor queries run periodically, and display changes update the requested resolution. Linux/Windows select the display with the greatest window overlap, including monitors with negative coordinates; macOS uses its reported primary display. High DPI is accounted for without changing the operating system's display mode. Fixed resolution presets and safe graphics mode remain available. Sandbox includes an integer level slider, tier presets, and a 0.25×–2× offline speed slider with a reset button. Online speed controls are disabled, and level slider requests are coalesced below server rate limits. Sliders retain their drag when the pointer leaves the track. Sandbox and server panels reserve space below arrival alerts; chat and tooltips occupy separate HUD regions.

Bots train real, small neural networks online using bounded replay buffers. Motion observations train aim prediction; visible class outcomes from humans and bots and their own loadout results train tank/build preferences. A contextual network chooses combat spacing, flanking, approach, and ally rally maneuvers from actual score, damage, kills, and survival outcomes. Exploration continues so alternatives are not permanently excluded. Strong melee tanks still commit at contact range, while recovery, objectives, visibility, legal movement, and map constraints take precedence.

Cooperation under shared threats builds personal friendship. Betrayal reduces friendship and increases fear/animosity; grudges cool down and memories are bounded. These relationships influence target preferences and support weighting without granting hidden information. Parry decisions use stable per-projectile mistakes and varied timing: intended missed-attempt rates are 48% / 28% / 18% / 10% for Beginner / Normal / Advanced / Expert, before aim, reaction delay, and physical evasion affect the outcome. Repeated frames cannot reroll the same threat into a guaranteed defense.

Learning survives deaths, and network weights carry into later rounds. Every bot in a game or on a server shares one memory: how many points each tank earns per minute alive, which tanks beat which, which upgrade paths lead to the strongest tanks, and the pooled weights of each bot's build, tactics and motion networks, merged in when a bot dies. Servers save it to `bot_memory_file` (see [Hosting](hosting.md)) and offline games to `bot-memory.json` in the data folder, so bots keep what they learned across restarts. Only public information goes in. Sandbox and modded matches teach nothing, since their tanks don't follow the usual rules. Player sightings, relationships and tactical trials still reset between rounds.

Bots also drive with a small trained network. Each tick it scores sixteen directions around the bot from what the bot can see along each one: how close the walls are, whether incoming shots cross that way, where its target and other tanks are, and which way it meant to go. The bot steers toward the best direction. The weights come from self-play (`tankyard-tools train-driver`), which keeps changes that win against bots without the network and counts scraping walls and standing still against them. Measured over nine modes against the older hand-written steering, network bots scored 15–55% more points a minute in every mode and died less in all but Co-op, while touching walls and idling about as often. Close threats such as drones and homing rounds still trigger the hand-written retreat.

Stat upgrades immediately update deployed class drones/minions: speed, damage, remaining durability fraction, and minion reload. Upgrades preserve damage already taken, entity identity, lifetime, and location. Captured parry returns retain their tier-specific caps; already-fired ordinary bullets retain their launch stats.

To inspect actual boss geometry without a GPU:

```sh
cargo run --locked -p tankyard-client --bin tankyard-models -- --bosses --columns 4 --output artifacts/models/bosses.svg
```

Add `--phase 2` or `--phase 3` to draw them wounded: a boss glows amber in its second phase and pulses red in its last.

## Fighting bosses in 2.0

A boss is as tough as the arena is strong. Its health follows how many players are in the arena and how high their levels are, and moves with them if they join or leave mid-fight, keeping how hurt the boss is. A fight lasts about as long for one veteran as for a party. Bosses that command swarms also launch fewer drones against fewer players. Everyone who hit a boss in the last minute shares its reward.

Before a special attack, a boss shows exactly where its rounds will go: lanes for a fan, spokes for a ring, and chevrons for a charge. Its core whitens as the attack charges. It fires faster as it's hurt.

The **Siege Engine's cannon** winds up for 1.5 seconds, drawing a red line to the first wall in its way, then fires a huge, very fast round that destroys any tank it reaches, shields included. Get off the line or get behind cover. Walls stop it, a perfect parry turns it back at a capped damage, and it can't one-shot another boss.

**Bastion's cannon** is the smaller relation. It winds up for 1.15 seconds behind a steel line, then fires a fast slug for 125 damage. That won't always kill you, but it will make you reconsider standing there. Walls stop it too.

**Railgunner** is the one that picks you because you thought you were safe. It hunts the furthest player it can see, leads them, and locks its aim for the last 0.55 seconds of each warning. Its barrel tells you what's coming: a rail shot barely moves it, while the plasma beam splits the armor, spreads the coils and unfolds the muzzle claws. After the beam it vents for about two seconds with its barrel open and takes 40% more damage. It's poor at close range, so getting under the gun is usually the right answer.

**Parrying Railgunner** requires facing the incoming rail or plasma stream and catching it within 75 milliseconds of starting the parry, inside a roughly 49° cone either side of your aim. Its smaller shots use the usual parry window. Other bosses retain their normal parry rules.

**Parrying a heavy shot** starts a clash. The strike runs back along the attack and takes a share of the boss's full health: 10% for the beam, 9% for Railgunner's cannon, 7% for Siege Engine and 4% for Bastion. Railgunner's lighter echo round is worth half a cannon clash, and bots deal 40% of a player's amount. One volley can only clash once.

Bosses aim where you're going, not where you were: they work out where a round will meet a moving tank. They drive in straight lines between decisions and would rather finish a wounded target than start on a healthy one. Running in a straight line is how you get hit. Changing direction is the counterplay.

## Boss combat in 2.1

Each ordinary boss now chooses between three special attacks according to range,
the crowd around it and the way its target moves. Later phases shorten its pauses
and increase the pressure. Warnings still precede dangerous attacks, and cover,
sharp changes of direction and well-timed defensive abilities remain useful.

The sound designs layer foley and vocal performances with their own attacks,
weight and room tails. Rail, plasma, burning shells and steel slugs have distinct
parry cues. Volume changes also apply to effects already playing. Exhaust keeps a
steady direction through stops and turns, with finer curves close to the camera.
Reduced Motion suppresses decorative shake, flashes and moving trails.

<details>
<summary>Secret encounter: Sunderworm</summary>

Sunderworm is a long, segmented creature with a three-second ground-shaking
entrance. Cracks, dust and thrown floor tiles mark where it is surfacing. Its
body can be hit or collide with tanks only where it is above ground. Watch the
lunge lane and eruption ring; retreat sideways before the jaws strike or the
floor opens.

Five phases introduce bone fans, bile, burrowing, larvae, spine waves and tighter
pursuit. At 16% health it tears into a head and tail, which fight independently.
Both must be destroyed. A surviving half becomes more aggressive. Its boss bar
shows phase boundaries and warns before lunges, eruptions and the split.

The jaws fold back to expose fixed, hooked teeth and a contracting throat. Torn
ends bleed in streams that break into falling drops and wet pools. Blood, flesh,
dust and thrown tiles use the arena effects renderer and its lighting; they
follow the Effects setting. Attack warnings remain visible with effects off.

An eligible natural boss arrival has about a one-in-two-million chance of being
Sunderworm, and requires an arena worth three full-level tanks. The Boss Rush
roster has a one-in-24,000 roll when opened. Once selected, it stays in your saved
lineup. Hosts may explicitly choose `sunderworm` in a dedicated-server lineup or
a map Boss trigger. A local pack can keep the card available with
`"menu": { "bosses": ["sunderworm"] }`.

Tank Yard 2.1.1 uses **protocol 35**, the same as 2.1.0. Update the host and its players together; those two releases can play together.
Snapshots carry its real body positions, including when joining mid-fight.

</details>

## v0.2.16 combat update

Bosses start at roughly 4,590–6,885 health before higher waves and participant scaling, with larger health reserves for defensive hulls. All fourteen bosses have a secondary attack cycle with a 0.9-second white-ring windup. These attacks complement their primary weapons: drone bosses add suppression fans, defensive bosses place trap rings with gaps, pursuers launch seeking volleys and charges, Sentinel fires an aimed piercing fan, Prism/Eclipse alternate patterned rings, and Foundry combines explosive fans with mine screens. Special attacks lock their aim during the warning. Each boss has a distinct silhouette with clean armor panels and mounts. All shots retain normal wall collision, finite lifetime, projectile caps, and applicable parry/stasis counterplay. The aggregate random encounter chance remains 0.25% per simulation second.

## v0.3.0 balance

Summoner drones acquire players within 820 units of the drone instead of 1000, stop selecting targets beyond 950 units of the boss, and return to guard once beyond a 1050-unit owner leash. Their damage is 18 instead of 20 and four-drone replenishment takes 0.72 seconds instead of 0.6. Swarm caps remain unchanged. Guardian, Fallen Overlord, and Foundry drones also have archetype-specific leashes and observe cover, spawn protection, and the boss's restraint toward distant players below level 15.

Every boss pauses primary fire during its special-attack warning and leaves at least 0.4 seconds after the secondary volley before its next primary. Fallen Booster's normal speed is 180 instead of 195 and front bullet damage is 19 instead of 21. Prism's primary rings fire every 1.45 seconds instead of 1.3, last 4.2 seconds instead of 5, and ricochet once instead of twice. Bastion mines deal 27 instead of 32 damage and last 6.5 instead of 8 seconds. Tempest fires every 1.05 seconds instead of 0.9, with a fan centered on its aim. The larger health reserves and aggregate encounter probability remain unchanged.

Siege Engine holds a firing lane with slow, heavy shells. Skirmisher circles at speed and telegraphs a straight charge. Trapper places two mine pockets with a clear lane between them. The three have dedicated offline practice encounters.
