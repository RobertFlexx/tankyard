# Server measurements

## v1.1.0 reliability checks

Measured on 2026-11-13 with Rust 1.100.0 release binaries, protocol 28, Linux and an Intel i7-9700. The server and load clients ran on the same machine, alongside local builds and UI checks.

- A 600-second run with eight clients, twelve fixed bots, 100 ms emulated delay, 20 ms jitter and 5% imposed loss finished with all eight clients ready, none stale and all 19 scheduled reconnects completed. The server reported no skipped snapshots or dropped catch-up ticks. The combined process peaked at about 19.3 MiB resident memory; its timing reservoir stayed at 65,536 samples for 279,494 observations.
- A 45-second loopback run with 64 clients split into four arenas finished with all 64 ready, none stale and 0.00% reported loss. Each arena kept 24 fixed bots. The server reported no skipped snapshots or dropped catch-up ticks. Its final status window measured average/p95/p99 simulation work of 279/213/620 µs, with a worst tick of 54.8 ms and a maximum snapshot batch of 6.15 ms. The combined process peaked at about 30.8 MiB resident memory. The timing spikes include contention with the other local work.

These checks cover sustained joins, snapshot delivery and reconnect recovery. They do not establish internet performance or production capacity. Keep the default player limit until measurements on the intended host support raising it.

Reproduce them with:

```sh
tankyard-load --local --clients 8 --bots 12 --fixed-bots --seconds 600 --latency-ms 100 --jitter-ms 20 --loss-percent 5 --reconnect-every 30 --output artifacts/soak.json
tankyard-load --local --clients 64 --bots 24 --fixed-bots --seconds 45 --output artifacts/64-clients.json
```

`--output` writes the measured results, transport settings, reconnect counts and server status as JSON. Mean and maximum client-update times use every observation; percentiles use a bounded uniform sample. See [hosting](hosting.md#scaling-arenas) for arena limits and deployment choices.

## Earlier measurements

Measured on 2026-10-15 with release builds, Rust 1.99.0, Linux, and an Intel i7-9700. These are local tests, not a hosting capacity guarantee.

The server reuses a bounded 512 KiB snapshot serialization buffer, client ID storage, and reliable-event storage. Join data is encoded once per content revision. Control broadcasts are encoded once, and shared message buffers avoid copying the same payload for every recipient. `status` counts skipped snapshots as well as dropped simulation ticks.

## Snapshot benchmark

Both runs use seed 42, 64 simulated players, 24 bots, 1,000 requested targets, roughly 3,000 replenished projectiles, 120 warmup ticks, and 600 measured ticks. Snapshot timings cover one viewer's interest filtering, serialization, and compression.

| Arena | Before / after average snapshot | Before / after allocation calls per snapshot | Allocation reduction |
| --- | --- | --- | --- |
| Open | 0.151 / 0.140 ms | 23.38 / 9.38 | 59.9% |
| Maze | 0.162 / 0.149 ms | 22.56 / 8.56 | 62.1% |

Compressed payload sizes stayed identical for the seeded workloads. A regression test also compares the reusable encoder's output byte for byte with the original encoder and checks recovery after oversized state.

Simulation rules and snapshot frequency are unchanged. These timings include normal run-to-run variation; allocation counts are the stronger evidence for reduced memory churn.

## UDP workloads

Each run lasts 10 seconds, with 24 bots and 180 configured targets. The same process runs the server and all load clients.

| Workload | Clients ready at finish | Stale clients | Skipped snapshots | Dropped simulation ticks | Reported transport packet loss |
| --- | --- | --- | --- | --- | --- |
| 64 clients, loopback | 64 | 0 | 0 | 0 | 0.91% |
| 128 clients, loopback | 128 | 0 | 0 | 0 | 34.68% |
| 8 clients, 100 ms latency / 20 ms jitter / 5% imposed loss | 8 | 0 | 0 | 0 | 4.73% |

The 128-client combined workload still shows substantial transport loss. It does not establish a lag-free 128-player production capacity. Retain the 64-player default, spread growth across arena processes, and measure separate clients against the intended hosting hardware before raising room limits. See [hosting](hosting.md#scaling-arenas).

A protocol 9 follow-up on 2026-10-16 ran 64 clients and 24 bots for 15 seconds. All clients stayed ready with none stale, 0 skipped snapshots, 0 dropped catch-up ticks, and 0.94% reported packet loss. The server reported average/p95/p99 ticks of 80.5/100/119 µs and a maximum snapshot batch of 3.925 ms. This repeat used ordinary FFA combat; it does not measure a full 64-player co-op wave.

Reproduce a snapshot workload with:

```sh
./target/release/tankyard-tools bench --players 64 --bots 24 --ticks 600 --map open --output artifacts/bench/central-open.json
./target/release/tankyard-tools bench --players 64 --bots 24 --ticks 600 --map maze --output artifacts/bench/central-maze.json
```

The benchmark uses the same reusable encoder as the server. The `artifacts/` folder is ignored by Git, so reports stay local.

## v0.2.9 follow-up

Measured on the same i7-9700 on 2026-10-16, for 15 seconds per run:

| Transport / workload | Ready at finish | Stale | Reported loss | Dropped ticks / skipped snapshots |
| --- | --- | --- | --- | --- |
| UDP, 64 clients + 24 fixed bots | 64 | 0 | 0.00% | 0 / 0 |
| UDP, 128 clients + 24 fixed bots | 128 | 0 | 0.00% | 0 / 0 |
| UDP, 8 clients, 100 ms delay / 20 ms jitter / 10% imposed loss | 8 | 0 | 9.78% | 0 / 0 |
| WebSocket relay, 32 clients + 24 fixed bots | 32 | 0 | 0.00% | 0 / 0 |

The 64-player run measured average/p95/p99 simulation ticks of 79.5/93/114 µs and a maximum snapshot batch of 3.693 ms. At 128 players these were 101.6/136/160 µs and 11.011 ms. The WebSocket run used separate server and relay processes on loopback. The impaired run used the default adaptive population, so it had no friendly arena bots once all eight clients joined.

Larger UDP buffers reduced local burst drops in these runs. This does not remove internet packet loss or TCP's delay when a WebSocket connection retransmits data. Keep the 64-player default and measure on the intended host; these short, single-machine runs do not establish production capacity.

Empty arenas stop simulation, bot learning, wave advancement, and round timers while their transport keeps accepting connections. Bot population scales to `min(bot_count, max(0, 6 - humans))`; co-op enemies are excluded. Use `--fixed-bots` with the load tool to reproduce a fixed population. The neural learner uses a bounded 6-input, 8-hidden-unit, 2-output model per bot, trained only at visible observations. It changes a small part of motion prediction and does not change combat stats.

Tests stall a WebSocket writer, confirm bounded backpressure, then resume and verify all 96 datagrams arrive in order.

## v0.2.11 follow-up

The same 64-client / 24-fixed-bot UDP workload ran for 15 seconds on the i7-9700 with the new progression, sight, evasion, and build-learning code. All 64 clients were ready at finish, none stale, with 0.00% reported packet loss, zero skipped snapshots, and zero dropped catch-up ticks. Simulation ticks averaged 82.4 µs, with p95 / p99 of 97 / 113 µs; the maximum snapshot batch took 3.915 ms. The combined server and load-client process used about 23 MiB resident memory.

The synthetic 64-player / 24-bot benchmark with roughly 3,000 replenished projectiles averaged 0.940 ms per simulation tick in Open and 6.719 ms in Maze. AI averaged 0.072 / 0.423 ms respectively. These short local samples cover simulation and networking, not production capacity or rendering performance.

Each bot has two additional eight-hidden-unit networks: an 8-input class-value model and a 13-input skill-allocation model, each with one output. They train from the bot's own results over two-second windows, with a maximum of 40 replay entries and three replay updates per outcome. Very short builds are discarded; deaths still count. The models only influence legal upgrade commands. The class model also learns from the results of up to eight continuously visible human players, discarding samples across hidden gaps. Skill allocations and controls remain private. A separate 12-opponent memory tracks fear and animosity, with decay over time and bounded changes to target choice, spacing, and retreat. These memories persist across lives and reset with the match. Idle arenas still stop simulation and learning.

## v0.2.12 follow-up

Measured on the same i7-9700 on 2026-10-17, with the level-55 catalog, input history, and deployable prediction state. Each network run lasted 15 seconds on an otherwise idle machine.

| Transport / workload | Ready at finish | Stale | Reported loss | Dropped ticks / skipped snapshots |
| --- | --- | --- | --- | --- |
| UDP, 64 clients + 24 fixed bots | 64 | 0 | 0.00% | 0 / 0 |
| UDP, 8 clients, 100 ms delay / 20 ms jitter / 10% imposed loss | 8 | 0 | 9.73% | 0 / 0 |
| WebSocket relay, 32 clients + 24 fixed bots | 32 | 0 | 0.00% | 0 / 0 |

The 64-client run measured average/p95/p99 simulation ticks of 85.8/100/121 µs, a maximum snapshot batch of 4.172 ms, and about 24 MiB resident memory for the combined server and client process. The WebSocket run used separate server and relay processes and averaged 71.5 µs per simulation tick. The impaired run used adaptive bots, with no arena bots after eight humans joined. These are short loopback and emulated-link checks, not production capacity guarantees.

The synthetic 64-player / 24-bot / roughly 3,000-projectile workload averaged 0.967 ms per tick in Open and 7.396 ms in Maze; p99 was 1.170 / 14.163 ms. AI averaged 0.076 / 0.526 ms. This heavier cover sample costs more than the v0.2.11 sample; timings include run-to-run variation and the expanded catalog. Parry interception calculations run only for tanks that can parry. The normal multiplayer workload remained well below its tick budget in these tests.

Each input packet carries its current frame and two recent frames, in a fixed bundle below 80 bytes. Sequence validation discards already applied inputs, while coalescing preserves short fire and ability taps with the newest movement. The 16 KiB client input queue, per-peer rate budget, and 64-packet receive limit remain bounded. Four authoritative defense counts let recoil prediction respect deployable limits even when older units are outside the snapshot's view. No reliable input backlog was added.
