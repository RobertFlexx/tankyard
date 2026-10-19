# Hosting

Build and start a community server:

```sh
cargo build --locked --release -p tankyard-server --bin tankyard-server
./target/release/tankyard-server --config configs/server.toml
```

**Security:** The example server runs without a `netcode_private_key`: this is the *unauthenticated* netcode mode, meant for local testing and private LAN games. Do not treat an ordinary server password as encryption or verified player identity. For internet deployment, use secure netcode tokens issued by a trusted backend as described below. The game does not ship a public account/token service.

The port is UDP 5000. Local players join `127.0.0.1:5000`; LAN players use the host's local address. Internet players need a reachable public address and a forwarded UDP port. IPv6 addresses use brackets, such as `[2001:db8::1234]:5000`.

Host server in the menu runs until you leave the session and assigns its local player the Owner role. A dedicated server runs independently. FFA and Maze continue indefinitely; finite matches restart after the configured delay. Simulation runs at 60 ticks per second, with 20 snapshots per second by default. Protocol 12 is required on both clients and servers. Validated mod data is sent when joining, so clients do not need to install server packs.

Inputs and menu commands are flushed before the client finishes each frame. Movement abilities use shared prediction rules. Remote movement can bridge a packet gap for up to 75 ms, constrained by walls and the arena boundary; longer gaps freeze until a new snapshot arrives. The server reuses snapshot storage between viewers and bounds catch-up work during overload.

Edit `configs/server.toml` for mode, map, population, password, chat, and storage. Choose a unique `server_id` before using persistent access files. A file from another server ID is rejected. Generate a complete configuration with `--write-config my-server.toml`. CLI options override it:

```sh
./target/release/tankyard-server --config my-server.toml --bind 0.0.0.0:5001 --mode domination --bots 16 --mod mods/vampire
```

Modes: `ffa`, `two-teams`, `four-teams`, `maze`, `domination`, `survival`, `tag`, `mothership`, `capture-core`, `boss-rush`, `deathmatch`, `sandbox`, `random-events`, and `coop-pve`. In Co-op PvE, the bot setting counts friendly teammates; waves add their own enemies. Sandbox tools need `--allow-sandbox`. Mods may provide custom maps and rules; see [modding](modding.md).

Current clients and servers use protocol 14. Proximity voice, spectator controls, and captured drones require matching versions. Update clients and servers together. The sample configs use 15 seconds of stationary spawn protection; existing configs keep their chosen duration. Aiming does not end it, and offensive actions wait until movement or expiry.

The process console is the host's administration interface:

| Command | Action |
| --- | --- |
| `status`, `players` | Match, timings, player IDs, and addresses |
| `role 123 owner` | Assign a community owner by player entity ID |
| `role 123 moderator` | Assign moderator, admin, or player |
| `bots 24`, `difficulty normal` | Change bots and difficulty |
| `mode domination` | Start another mode |
| `maps`, `map maze` | List loaded maps or select a map and start a new round |
| `restart` | Start a new round with the current settings |
| `kick 123`, `ban 123 60` | Remove a player or ban their address for 60 minutes |
| `unban 192.168.1.20`, `bans` | Manage address bans |
| `reload-mods` | Validate packs, then start a new round; a failed load preserves the current round |
| `save my-server.toml`, `shutdown` | Save settings or stop cleanly |

The in-game Server panel appears only to players with server permissions. Its Settings tab shows the server's current mode, map, bot population, and difficulty. The Settings permission allows changing the match and restarting a round. Bot population changes apply immediately; applying mode, map, and difficulty starts a new round while keeping players connected and retaining their server roles. A plain restart uses the current settings. Unknown maps and Sandbox selection without `allow_sandbox` are rejected before replacing the match. Maze mode with map `open` selects map `maze` automatically. Console `status` includes the active map, round, and difficulty; commands reject missing or extra arguments.

Owners can nominate lower roles and choose Kick, Ban, Mute, Manage rooms, Manage roles, Settings, Reload mods, and View audit individually. Delegates cannot grant permissions they lack, appoint an owner, or moderate someone of equal or higher authority. Every request is checked on the server and rate limited. Gameplay mods have no administration API.

Anonymous grants last for the connection. Persistent roles and account bans/mutes require a verified account, an installed account verifier, and authenticated transport. Names and client IDs never prove account ownership. Address bans work without accounts. Anonymous mutes end on disconnect. `access_file` stores verified grants and sanctions; `audit_file` stores bounded, rotating community logs. New operator files use private permissions on Unix and atomic replacement. Audit logging is optional; the sample config enables it.

Community servers can enable chat and up to eight rooms. Rooms can restrict access by role, keep 32 messages, and limit sending to three initial messages followed by one every two seconds. Enter opens chat; Escape closes it. Chat keeps full messages with wrapped lines and mouse-wheel history. Rejoining a room does not duplicate messages. Official servers always reject chat and do not expose community role controls, even if their configuration enables chat.

`voice_enabled = true` enables proximity voice independently of text chat on either server kind. Set it to false to disable voice. Voice uses the existing game connection and relay, so no extra ports or voice service are needed. The server supplies the sender identity and distance attenuation, and forwards audio only between living players within 900 units. Moderator mutes also block voice, including modified clients. Audio uses bounded 20 ms mono frames on a separate unreliable channel, capped at 50 frames per second with a small burst allowance; old rounds, duplicate sequences, oversized frames, and excess traffic are discarded. One continuously speaking player sends about 16 KiB/s before transport overhead, multiplied by the number of nearby listeners on server egress. Silence is gated at the client. Voice is not recorded or persisted by Tank Yard.

Dead players can follow living tanks; team modes restrict targets to teammates. Spectator requests are checked against the current round and authoritative life/team state. Snapshots and effects follow the selected tank's location, and return to the player's own location on respawn.

Set `kind = "Official"` for an official mode server. `configs/official.toml` is an accountless policy example. Accounts, matchmaking, the public server directory, and cloud provisioning remain separate services. Their integration points are `AccountVerifier`, `AccountContext`, `VerifiedAccount`, and `GlobalAuthority` in `tankyard_server::access`. Verification must check signed claims against the destination server and client ID, use cached revocation state, and avoid blocking network calls on the game thread. Override `AccountVerifier::is_current` to check cached revocations; active claims are rechecked once per second and expired or revoked sessions are removed.

For authenticated connections, set a private 32-byte hexadecimal `netcode_private_key` and `public_addresses` matching the addresses in issued tokens. Install the verifier before players join, then enable `require_accounts`. Missing verifiers reject account credentials. Use a fresh random client ID for each connection. Backend kicks use that connection ID; community moderation requests also include the current round to reject stale targets. The backend issues short-lived tokens with `accounts::issue_connection_token`; clients use `NetworkClient::connect_authenticated` or `tankyard --connect-token session.token --account-token-file account.token`. Distribute tokens through your authenticated service. Server keys stay private and separate for each server.

Global staff authority is a separate trusted backend gateway, restricted to official servers with authenticated transport and a private `global_audit_file`. It supports account suspension, official player removal, and community directory delisting. Staff credentials, roles, and actions have no client packet or user-facing panel. A community role cannot grant global access. The gateway must enforce revocation and propagate account suspensions to the other official servers. Global and community audit paths must differ.

To publish a server behind an HTTP-only proxy such as a Cloudflare Tunnel hostname, run `tankyard-relay --listen 0.0.0.0:8080 --server 127.0.0.1:5000` and route the hostname to port 8080. Players join `wss://host` or just `host`; the client carries netcode packets over the WebSocket. Every player then shares the relay's address, so use `kick` or a password rather than address bans. The relay log shows each player's original IP when Cloudflare forwards it.

Use `scripts/host_server.sh` for a convenience launcher. `configs/tankyard.service` runs under a dedicated user at `/opt/tankyard`, restarts on failure, and handles SIGTERM. Adjust paths and allow that user to write the configuration directory. Use `--no-console` under a service manager. Logs use `RUST_LOG`.

Check a running server on a machine with UDP access:

```sh
cargo run --locked --release -p tankyard-server --bin tankyard-load -- --local --clients 8 --seconds 15
```

The load tool also supports `--latency-ms 100 --jitter-ms 20 --loss-percent 5`. It fails if clients cannot join, disconnect before the finish, or stop receiving snapshots for more than two seconds.

## Scaling arenas

`adaptive_bots = true` is the default for official and community servers. `bot_count` is a ceiling: the arena fills to six participants, with zero bots at six human players. Co-op wave enemies are separate. With no humans connected, the simulation and round timers pause while networking continues to accept joins. Use `adaptive_bots = false` or `--fixed-bots` for a fixed bot population.

Realtime input bursts from delayed connections are coalesced. The server retains bounded processing and drops excess input instead of kicking a player for packet bunching. Malformed input and control-message limits still apply. UDP buffers absorb short bursts where the operating system allows it; WebSocket queues are bounded and pause UDP reads when TCP is blocked.

Run each arena in its own server process. That lets separate CPU cores and machines handle additional rooms. The central lobby should send players to an arena's UDP address and issue its connection token before they join. Account, database, and matchmaking requests belong outside the simulation thread; account verifiers should read cached claims and revocations.

Start with the current 64-player room limit and measure on the hosting hardware before increasing it. At 60 ticks per second, each tick has 16.7 ms; snapshot batches and network work need CPU time too. Monitor `status` for tick percentiles, snapshot time, skipped snapshots, and dropped catch-up ticks. Add rooms while there is still CPU and network headroom. The configured player limit alone does not establish capacity.

`configs/tankyard@.service` runs named arenas under systemd. For example, `tankyard@ffa-1` reads `/opt/tankyard/configs/ffa-1.toml`. Give each arena its own `server_id`, bind port, moderation files, and audit files. Set `public_addresses` and token issuance to match that arena. Install the template and prepare the `tankyard` user and paths as for the single-server unit, then start the required instances.

Repeat these local workloads after server changes:

```sh
./target/release/tankyard-load --local --clients 64 --bots 24 --fixed-bots --seconds 30
./target/release/tankyard-load --local --clients 128 --bots 24 --fixed-bots --seconds 30
./target/release/tankyard-load --local --clients 8 --seconds 30 --latency-ms 100 --jitter-ms 20 --loss-percent 5
```

The load process also simulates every client, so its CPU use and loopback packet loss differ from a deployed fleet. Check [measured results](status.md), then test the intended map, classes, and projectile population on the actual host. The central lobby and provisioning service remain separate integrations.

Creator badges use a per-connection challenge and Ed25519 proof checked against the public creator key. Display names and community roles do not confer creator identity. Standard builds have no owner-extension handler. Trusted hosts can register their own extension before accepting players; every extension packet carries a fresh signature bound to the connection nonce, current round, monotonic sequence, and exact payload. Bounded payloads, replay checks, rate limits, and audit logging apply before host code runs. Modified matches disable regular arena achievements.
