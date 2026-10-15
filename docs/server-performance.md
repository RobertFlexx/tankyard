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

Reproduce a snapshot workload with:

```sh
./target/release/tankyard-tools bench --players 64 --bots 24 --ticks 600 --map open --output artifacts/bench/central-open.json
./target/release/tankyard-tools bench --players 64 --bots 24 --ticks 600 --map maze --output artifacts/bench/central-maze.json
```

The benchmark now uses the same reusable encoder as the server. The saved `before` reports were captured before that change.
