# Server measurements

Measured on 2026-10-15 with release builds, Rust 1.99.0, Linux, and an Intel i7-9700. These are local tests, not a hosting capacity guarantee.

The server now reuses a bounded 512 KiB snapshot serialization buffer, client ID storage, and reliable-event storage. Join data is encoded once per content revision. Control broadcasts are encoded once, and shared message buffers avoid copying the same payload for every recipient. `status` counts skipped snapshots as well as dropped simulation ticks.

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

Logs and benchmark JSON are written under `artifacts/bench/`: `before-central-{open,maze}.json`, `after-central-{open,maze}.json`, `before-central-udp{64,128}.txt`, `after-central-udp{64,128}.txt`, and `after-central-impaired.txt`. Generated artifacts are gitignored.

A protocol 9 follow-up on 2026-10-16 ran 64 clients and 24 bots for 15 seconds. All clients stayed ready with none stale, 0 skipped snapshots, 0 dropped catch-up ticks, and 0.94% reported packet loss. The server reported average/p95/p99 ticks of 80.5/100/119 µs and a maximum snapshot batch of 3.925 ms. This repeat used ordinary FFA combat; it does not measure a full 64-player co-op wave.

Reproduce a snapshot workload with:

```sh
./target/release/tankyard-tools bench --players 64 --bots 24 --ticks 600 --map open --output artifacts/bench/central-open.json
./target/release/tankyard-tools bench --players 64 --bots 24 --ticks 600 --map maze --output artifacts/bench/central-maze.json
```

The benchmark now uses the same reusable encoder as the server. The saved `before` reports were captured before that change.

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

Empty arenas now stop simulation, bot learning, wave advancement, and round timers while their transport keeps accepting connections. Bot population scales to `min(bot_count, max(0, 6 - humans))`; co-op enemies are excluded. Use `--fixed-bots` with the load tool to reproduce a fixed population. The neural learner uses a bounded 6-input, 8-hidden-unit, 2-output model per bot, trained only at visible observations. It changes a small part of motion prediction and does not change combat stats.

Reports: `artifacts/bench/v029-{udp64,udp128,impaired,ws32}.txt` and `v029-ws-server.txt`. Tests also stall a WebSocket writer, confirm bounded backpressure, then resume and verify all 96 datagrams arrive in order.

## v0.2.11 follow-up

The same 64-client / 24-fixed-bot UDP workload ran for 15 seconds on the i7-9700 with the new progression, sight, evasion, and build-learning code. All 64 clients were ready at finish, none stale, with 0.00% reported packet loss, zero skipped snapshots, and zero dropped catch-up ticks. Simulation ticks averaged 82.4 µs, with p95 / p99 of 97 / 113 µs; the maximum snapshot batch took 3.915 ms. The combined server and load-client process used about 23 MiB resident memory. Report: `artifacts/bench/v0211-udp64.txt`.

The synthetic 64-player / 24-bot benchmark with roughly 3,000 replenished projectiles averaged 0.940 ms per simulation tick in Open and 6.719 ms in Maze. AI averaged 0.072 / 0.423 ms respectively. Reports are `artifacts/bench/v0211-after-{open,maze}.json`. These short local samples cover simulation and networking, not production capacity or rendering performance.

Each bot now has two additional eight-hidden-unit networks: an 8-input class-value model and a 13-input skill-allocation model, each with one output. They train from the bot's own results over two-second windows, with a maximum of 40 replay entries and three replay updates per outcome. Very short builds are discarded; deaths still count. The models only influence legal upgrade commands. The class model also learns from the results of up to eight continuously visible human players, discarding samples across hidden gaps. Skill allocations and controls remain private. A separate 12-opponent memory tracks fear and animosity, with decay over time and bounded changes to target choice, spacing, and retreat. These memories persist across lives and reset with the match. Idle arenas still stop simulation and learning.
