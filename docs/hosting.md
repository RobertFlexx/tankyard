# Hosting

Anyone can host a Tank Yard server: from the game in a few clicks, or as a dedicated server on a spare PC, a Raspberry Pi or a VPS. Pick the way that suits you:

| You want | Use | Time |
| --- | --- | --- |
| To play with friends tonight | [From the game](#from-the-game) | 1 minute |
| A server that runs without the game open | [A dedicated server](#a-dedicated-server) | 5 minutes |
| It to start with the machine and restart if it stops | [Run it as a service](#run-it-as-a-service) | 10 minutes |
| Players to join `play.example.com`, or no port forwarding | [Your own domain](hosting-domain.md) | 20 minutes |

Players never install your mods: the server sends its checked mod data when they join. Clients and the server must use compatible protocol versions. Tank Yard 1.1.0 uses protocol 28; clients and servers must both update. Version 1.0.0 used protocol 26, and 0.15 and 0.16 used 25.

## From the game

Open **Host server → Community server**. Name the server, pick a UDP port, maximum players, lobby size and bots per lobby, choose the mode and rules with **Configure arena**, and press **Start community server**. It runs as its own process, so it keeps going while you play anywhere, including on it (**Join**). The panel shows its live status (players, lobbies, mods and your connection to it), the address to share, and the server console. Your enabled mods load on the server.

**Copy command for an always-on machine** saves the configuration and copies the exact command that runs the same server without the game, for a spare PC, a VPS, or a Raspberry Pi.

Servers answer status queries, so anyone who saves yours sees its name, player count, lobbies, mods, whether it needs a password, and a measured connection quality in **Join server**. Clicking the main server card on the title screen lists its lobbies; picking one joins that lobby.

## A dedicated server

### 1. Install it

On Linux (x86-64, or 64-bit ARM such as a Raspberry Pi 4 or 5 on 64-bit Raspberry Pi OS) and macOS:

```sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh -s -- --server
```

This puts `tankyard-server` and `tankyard-relay` in `~/.local/bin` and doesn't touch an installed game. Run the same command to update. On Windows, download the Windows ZIP from the [latest release](https://github.com/RobertFlexx/tankyard/releases/latest) and extract it; `tankyard-server.exe` is inside. Every release archive also contains the server and sample configs. Linux and macOS archives include a `host-server.sh` launcher, so extracting one is enough too.

On FreeBSD 15 or newer (amd64), use the base system's `fetch` instead of curl: `fetch -o - https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh -s -- --server`.

Linux builds need glibc 2.31 or newer on x86-64 and 2.35 or newer on ARM64. 32-bit ARM and musl systems such as Alpine need a [source build](#from-the-command-line).

### 2. Configure it

Make a folder for the server's settings and write a complete settings file:

```sh
mkdir -p ~/tankyard-server && cd ~/tankyard-server
tankyard-server --write-config server.toml --name "My arena"
```

On Windows, run `.\tankyard-server.exe --write-config server.toml --name "My arena"` in the extracted folder. Open `server.toml` in a text editor. The settings most hosts change:

| Setting | What it does |
| --- | --- |
| `name` | The name in **Join server** |
| `server_id` | A short unique id such as `"my-arena"`; needed before roles and bans can be saved |
| `password` | Players must type it to join. It keeps strangers out; it doesn't encrypt anything |
| `bind` | The UDP address and port, `"0.0.0.0:5000"` by default |
| `max_players`, `arena_capacity` | Players in total, and per lobby; a full lobby opens another |
| `bot_count`, `adaptive_bots`, `difficulty` | Bots per lobby, whether they leave as players join, and their skill |
| `mods` | Mod pack folders or ZIPs to load, such as `["mods/my_arena.zip"]` |
| `mode`, `map` under `[world]` | The game mode and map |

Command-line options override the file for one run: `--port 5001`, `--mode maze`, `--bots 8`, `--mod mods/vampire`. `tankyard-server --help` lists them all.

### 3. Start it

```sh
tankyard-server --config server.toml
```

It prints where players can join:

```text
My arena listening on UDP 0.0.0.0:5000 | Free For All | 60 Hz simulation, 20 Hz snapshots
Players join: 127.0.0.1:5000 on this computer
              192.168.1.20:5000 on your local network
              your public address, port 5000, once UDP 5000 is forwarded to this machine
```

Type `help` in the same window for the administration commands; `status` shows players and tick times, `save` writes changed settings back, and `shutdown` stops it cleanly. Players join with **Join server → Add**, entering the address.

### 4. Let players in

- **Same network:** players use the local address. If they can't connect, allow UDP 5000 through this machine's firewall:
  - Ubuntu and Debian with ufw: `sudo ufw allow 5000/udp`
  - Fedora and others with firewalld: `sudo firewall-cmd --permanent --add-port=5000/udp && sudo firewall-cmd --reload`
  - Windows (PowerShell as administrator): `New-NetFirewallRule -DisplayName "Tank Yard" -Direction Inbound -Protocol UDP -LocalPort 5000 -Action Allow`
  - macOS asks to allow incoming connections the first time; choose Allow.
  - FreeBSD with pf: add `pass in proto udp to port 5000` to `/etc/pf.conf`, then `service pf reload`.
- **Over the internet:** in your router's settings, forward **UDP** port 5000 to this machine's local address, and give the machine a reserved address so it doesn't change. Players join your public address, such as `203.0.113.7:5000`. IPv6 addresses use brackets: `[2001:db8::1234]:5000`.
- **Can't forward ports?** Some connections share one public address between many homes (carrier-grade NAT): the address your router shows differs from the one websites report. Use a [Cloudflare Tunnel](hosting-domain.md#option-b-a-cloudflare-tunnel) instead, which needs no open ports.

### Run it as a service

On Linux with systemd, the server can start with the machine and restart if it ever stops. The bundled unit runs it as its own `tankyard` user, with the program in `/opt/tankyard` and its settings, bans and logs in `/var/lib/tankyard`:

```sh
# A user for the server, and the program.
sudo useradd --system --no-create-home --shell /usr/sbin/nologin tankyard
curl -fsSLO https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh
sudo sh install.sh --server --bin-dir /opt/tankyard

# Its settings, owned by that user.
sudo install -d -o tankyard -g tankyard /var/lib/tankyard
sudo -u tankyard /opt/tankyard/tankyard-server --write-config /var/lib/tankyard/server.toml --name "My arena"
sudoedit /var/lib/tankyard/server.toml

# The service.
sudo curl -fsSL -o /etc/systemd/system/tankyard.service \
  https://raw.githubusercontent.com/RobertFlexx/tankyard/main/configs/tankyard.service
sudo systemctl daemon-reload
sudo systemctl enable --now tankyard
```

The unit is also in every release archive as `configs/tankyard.service`. Relative paths in the settings, such as `bans.json` or `mods/my_arena.zip`, are inside `/var/lib/tankyard`. Useful commands:

| Command | Does |
| --- | --- |
| `systemctl status tankyard` | Whether it's running |
| `journalctl -u tankyard -f` | Follow its log, including the status line |
| `sudo systemctl restart tankyard` | Restart after editing the settings |
| `sudo sh install.sh --server --bin-dir /opt/tankyard && sudo systemctl restart tankyard` | Update to the latest release |

Under a service there is no console; edit the settings file and restart instead, or use the in-game **Server** panel as an owner. Back up `/var/lib/tankyard` before updates.

### On FreeBSD

The release's `configs/tankyard.rc` is an rc.d script. It runs the server as its own `tankyard` user, supervised by `daemon(8)`, which restarts it three seconds after it stops. The program goes in `/usr/local/bin`, its settings, bans and mods in `/var/db/tankyard`, and its log in `/var/log/tankyard/server.log`. As root:

```sh
# A user for the server, and the program.
pw useradd tankyard -d /var/db/tankyard -s /usr/sbin/nologin -c "Tank Yard server"
fetch https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh
sh install.sh --server --bin-dir /usr/local/bin

# Its settings, owned by that user.
install -d -o tankyard -m 750 /var/db/tankyard
su -m tankyard -c 'tankyard-server --write-config /var/db/tankyard/server.toml --name "My arena"'
vi /var/db/tankyard/server.toml

# The service.
fetch -o /usr/local/etc/rc.d/tankyard https://raw.githubusercontent.com/RobertFlexx/tankyard/main/configs/tankyard.rc
chmod 555 /usr/local/etc/rc.d/tankyard
sysrc tankyard_enable=YES
service tankyard start
```

Relative paths in the settings are inside `/var/db/tankyard`. Useful commands:

| Command | Does |
| --- | --- |
| `service tankyard status` | Whether it's running |
| `tail -f /var/log/tankyard/server.log` | Follow its log, including the status line |
| `service tankyard restart` | Restart after editing the settings |
| `sh install.sh --server --bin-dir /usr/local/bin && service tankyard restart` | Update to the latest release |

`sysrc` changes the defaults: `tankyard_dir`, `tankyard_config`, `tankyard_user`, `tankyard_server` (the program) and `tankyard_args` (more `tankyard-server` options, such as `"--port 5001"`). To run several servers, name them, such as `sysrc tankyard_profiles="ffa teams"`. Each reads `/var/db/tankyard/<name>.toml` and logs to `/var/log/tankyard/<name>.log`, so give each file its own port. Stop the service before you change `tankyard_profiles`, so no server is left running under the old setting. `service tankyard restart ffa` acts on one of them.

To rotate the log, add this line to `/etc/newsyslog.conf.d/tankyard.conf`. `daemon(8)` then reopens the log file without restarting the server:

```text
/var/log/tankyard/server.log  tankyard:tankyard  600  7  1000  *  J  /var/run/tankyard/server.pid
```

### Raspberry Pi and small machines

Use a Raspberry Pi 4 or 5 with 64-bit Raspberry Pi OS and the ARM64 build. Start with the defaults, then watch `status` (or the journal) while people play: the `tick average/p95/p99/max` figures should stay well under 16,667 µs, the time one tick has at 60 ticks a second. If p99 creeps above about half of that, lower `bot_count`, `[world] target_count` (shapes) or `max_players`, or keep `snapshot_rate` at 20. Maze maps and boss modes cost the most. Run one server per machine at first and add arenas once you know there is headroom.

### Troubleshooting

| Problem | Try |
| --- | --- |
| Nobody can connect, even on the same network | Allow UDP on the server's port in its firewall (see above). Check the address and port the server printed |
| Local players can join, internet players can't | Forward **UDP**, not TCP, to the server's local address. Check that your router's WAN address matches your public address; if not, use a [Cloudflare Tunnel](hosting-domain.md#option-b-a-cloudflare-tunnel) |
| "Address already in use" | Another server uses that port. Stop it, or start this one with `--port 5001` |
| Players see an update message | Check the compatibility note above. Update the older client or server when their protocol versions differ |
| A mod or map is refused when the server starts | The log says why. Maps that use newer workshop logic need a server at least as new as the workshop that made them |
| The game stutters for everyone | Check `status`. High tick times or dropped catch-up ticks mean the machine is overloaded: lower bots, shapes or players. Many skipped snapshots with low tick times mean connections are congested, often the host's upload: lower `snapshot_rate` or `max_players` |
| Roles and bans are forgotten after a restart | Set a `server_id` and make sure the server can write its `access_file` and `ban_file` |
| The service won't start | `journalctl -u tankyard -e` shows the error. Check that `/var/lib/tankyard/server.toml` exists and belongs to the `tankyard` user. On FreeBSD, `service tankyard start` and `/var/log/tankyard/server.log` say why |

For a public hostname such as `play.example.com`, follow the [domain, DNS and HTTPS guide](hosting-domain.md). The server includes an optional WebSocket listener, so a separate relay process is not needed.

## From the command line

To build the server from source instead of downloading it:

```sh
cargo build --locked --release -p tankyard-server --bin tankyard-server
./target/release/tankyard-server --config configs/server.toml
```

**Transport:** The default UDP server accepts guests. A server password does not encrypt the link or verify player identity. For a public domain, use the [WSS setup](hosting-domain.md) to protect the connection through TLS. For token-authenticated UDP invitations, use the setup below. Verified accounts still require an account service.

The port is UDP 5000. Local players join `127.0.0.1:5000`; LAN players use the host's local address. Internet players need a reachable public address and a forwarded UDP port. IPv6 addresses use brackets, such as `[2001:db8::1234]:5000`.

Host server in the menu runs until you leave the session and assigns its local player the Owner role. A dedicated server runs independently. FFA and Maze continue indefinitely; finite matches restart after the configured delay. Simulation runs at 60 ticks per second, with 20 snapshots per second by default; `snapshot_rate` takes 1 to 60, and higher rates are smoother at the cost of bandwidth. Tank Yard 1.1.0 uses protocol 28 and older clients must update before joining. Validated mod data is sent when joining, so clients do not need to install server packs.

Inputs are sent as soon as each frame's update finishes, before drawing; menu commands go out before the frame ends. Movement abilities use shared prediction rules. Remote movement can bridge a packet gap for up to 75 ms, constrained by walls and the arena boundary; longer gaps freeze until a new snapshot arrives. The server reuses snapshot storage between viewers and bounds catch-up work during overload.

Edit `configs/server.toml` for mode, map, population, password, chat, and storage. Choose a unique `server_id` before using persistent access files. A file from another server ID is rejected. Generate a complete configuration with `--write-config my-server.toml`. CLI options override it; `--port 5001` listens on every interface at that port, and `--arena-capacity 20` splits players into lobbies of 20:

```sh
./target/release/tankyard-server --config my-server.toml --bind 0.0.0.0:5001 --mode domination --bots 16 --mod mods/vampire
```

Modes: `ffa`, `two-teams`, `four-teams`, `maze`, `domination`, `survival`, `tag`, `mothership`, `capture-core`, `boss-rush`, `deathmatch`, `sandbox`, `random-events`, and `coop-pve`. In Co-op PvE, the bot setting counts friendly teammates; waves add their own enemies. Sandbox tools need `--allow-sandbox`. Mods may provide custom maps and rules; see [modding](modding.md).

Tank Yard 1.1.0 uses protocol 28, which carries Bacteria's pursuing doubles and the workshop's new effects. Version 1.0.1 used 27, 1.0.0 used 26, and 0.15 and 0.16 used 25. Upgrade clients, servers and any separate relay together; older snapshots are incompatible. Older clients receive an update message. The sample configs use 15 seconds of stationary spawn protection; existing configs keep their chosen duration. Aiming does not end it, and offensive actions wait until movement or expiry.

The ordinary server includes signed creator tools by default on official public arenas (`--central`); `creator_admin = false` overrides that policy. Default community/personal hosts are disabled; public community operators can explicitly opt in. Password-protected hosts cannot enable the tools. See [creator administration](creator-administration.md) for policy, immediate revocation, auditing, and updating the central Raspberry Pi/Docker deployment.

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
| `rules`, `rules movement_multiplier 1.2`, `rules reset` | Read rules, change one value live, or restore defaults |
| `kick 123`, `ban 123 60` | Remove a player or ban their address for 60 minutes |
| `unban 192.168.1.20`, `bans` | Manage address bans |
| `reload-mods` | Validate packs, then start a new round; a failed load preserves the current round |
| `save my-server.toml`, `shutdown` | Save settings or stop cleanly |

The in-game Server panel appears only to players with server permissions. Its **Rules** tab changes movement, acceleration, coasting, damage, health, regeneration, XP, score and respawn time without a restart. Health preserves each tank's current percentage; existing bullets keep their damage. The Configure permission is required, and the change applies to every arena. `save` persists an explicit `rules_override` profile, taking priority over a pack's initial rules on later process restarts. Modified rounds do not earn standard achievements.

Its Settings tab shows the server's current mode, map, bot population, and difficulty. The Settings permission allows changing the match and restarting a round. Bot population changes apply immediately; applying mode, map, and difficulty starts a new round while keeping players connected and retaining their server roles. A plain restart uses the current settings. Unknown maps and Sandbox selection without `allow_sandbox` are rejected before replacing the match. Maze mode with map `open` selects map `maze` automatically. Console `status` includes the active map, round, and difficulty; commands reject missing or extra arguments.

Owners can nominate lower roles and choose Kick, Ban, Mute, Manage rooms, Manage roles, Settings, Reload mods, and View audit individually. Delegates cannot grant permissions they lack, appoint an owner, or moderate someone of equal or higher authority. Every request is checked on the server and rate limited. Gameplay mods have no administration API.

Anonymous grants last for the connection. Persistent roles and account bans/mutes require a verified account, an installed account verifier, and authenticated transport. Names and client IDs never prove account ownership. Address bans work without accounts. Anonymous mutes end on disconnect. `access_file` stores verified grants and sanctions; `audit_file` stores bounded, rotating community logs. New operator files use private permissions on Unix and atomic replacement. Audit logging is optional; the sample config enables it.

Community servers can enable chat and up to eight rooms. Rooms can restrict access by role, keep 32 messages, and limit sending to three initial messages followed by one every two seconds. Enter opens chat; Escape closes it. Chat keeps full messages with wrapped lines and mouse-wheel history. Rejoining a room does not duplicate messages. Official servers keep text chat disabled. The verified central owner and their delegated staff can use server-local moderation on the central profile; other official servers do not expose community role controls. See [server administration](creator-administration.md).

`voice_enabled = true` enables proximity voice independently of text chat on either server kind. Set it to false to disable voice. Voice uses the existing game connection and relay, so no extra ports or voice service are needed. The server supplies the sender identity and distance attenuation, and forwards audio only between living players within 900 units. Moderator mutes also block voice, including modified clients. Audio uses bounded 20 ms mono frames on a separate unreliable channel, capped at 50 frames per second with a small burst allowance; old rounds, duplicate sequences, oversized frames, and excess traffic are discarded. One continuously speaking player sends about 16 KiB/s before transport overhead, multiplied by the number of nearby listeners on server egress. Silence is gated at the client. Voice is not recorded or persisted by Tank Yard.

Dead players can follow living tanks; team modes restrict targets to teammates. Spectator requests are checked against the current round and authoritative life/team state. Snapshots and effects follow the selected tank's location, and return to the player's own location on respawn.

Set `kind = "Official"` for an official mode server. `configs/official.toml` is an accountless policy example. Accounts, matchmaking, the public server directory, and cloud provisioning remain separate services. Their integration points are `AccountVerifier`, `AccountContext`, `VerifiedAccount`, and `GlobalAuthority` in `tankyard_server::access`. Verification must check signed claims against the destination server and client ID, use cached revocation state, and avoid blocking network calls on the game thread. Override `AccountVerifier::is_current` to check cached revocations; active claims are rechecked once per second and expired or revoked sessions are removed.

### Token-authenticated UDP

Create a new private config with the public IP and UDP port that players reach:

```sh
tankyard-server --secure --public-address 203.0.113.10:5000 --write-config community-private.toml
tankyard-server --config community-private.toml --check-config
tankyard-server --config community-private.toml --no-console
```

Replace the example address with your own. The first command generates a fresh server key without printing it. It refuses existing files and configs that already have a key. Keep the config on the server; on Unix it is created with owner-only permissions. On Windows, store it in a folder restricted to the server account. UDP port forwarding and firewall rules still apply.

In another terminal, create a five-minute invitation:

```sh
tankyard-server --config community-private.toml --issue-token friend.token --token-seconds 300
```

Send only `friend.token` through a private, trusted channel. The player joins with:

```sh
tankyard --connect-token friend.token
```

Each invitation gets a random connection ID. Existing invitation files are never overwritten. `--token-seconds` accepts 10–3600 seconds; issue another invitation for another connection. The server key stays in its config. Use the domain/WSS setup for ordinary hostname joins; the invitation command is for UDP endpoints.

For verified accounts, install an `AccountVerifier` before players join, then enable `require_accounts`. Missing verifiers reject account credentials. An account backend issues short-lived tokens with `accounts::issue_connection_token`; clients use `NetworkClient::connect_authenticated` or `tankyard --connect-token session.token --account-token-file account.token`. Backend kicks use the connection ID; community moderation requests also include the current round to reject stale targets. Keep keys private and separate for each server.

Global staff authority is a separate trusted backend gateway, restricted to official servers with authenticated transport and a private `global_audit_file`. It supports account suspension, official player removal, and community directory delisting. Staff credentials, roles, and actions have no client packet or user-facing panel. A community role cannot grant global access. The gateway must enforce revocation and propagate account suspensions to the other official servers. Global and community audit paths must differ.

To publish a server behind an HTTP-only proxy such as a Cloudflare Tunnel hostname, set `websocket = "127.0.0.1:8080"` in the game server config and route the hostname to port 8080. The standalone `tankyard-relay` remains available for separate-process deployments. Players join `wss://host` or just `host`; the client carries netcode packets over the WebSocket. The server scopes loopback bans to a connection, so banning one proxied player cannot exclude everyone on that relay. Lasting individual bans for anonymous guests behind a shared proxy still need verified accounts. The relay log shows each player's original IP when Cloudflare forwards it.

`host-server.sh` in a release folder (`scripts/host_server.sh` in the source) starts the server with that folder's `server.toml`, writing it from the sample on the first run. `configs/tankyard.service` runs the server as a service; see [Run it as a service](#run-it-as-a-service). It restarts on failure and stops cleanly on SIGTERM. Use `--no-console` under any service manager. Logs use `RUST_LOG`, such as `RUST_LOG=debug`.

Check a running server on a machine with UDP access:

```sh
cargo run --locked --release -p tankyard-server --bin tankyard-load -- --local --clients 8 --seconds 15
```

The load tool also supports `--latency-ms 100 --jitter-ms 20 --loss-percent 5`. It fails if clients cannot join, disconnect before the finish, or stop receiving snapshots for more than two seconds.

For a longer run with reconnects and a JSON report:

```sh
tankyard-load --local --clients 8 --seconds 600 --latency-ms 100 --jitter-ms 20 --loss-percent 5 --reconnect-every 30 --output soak.json
```

Runs can last up to 24 hours. Timing samples stay bounded; mean and maximum use every observation, while p95 and p99 use a uniform sample of up to 65,536 observations. The report includes completed reconnects, stale clients and peak combined resident memory where the OS exposes it. Reconnect tests against a remote host need a spare connection slot while a lost disconnect expires. Loopback results do not establish internet or production capacity.

## Scaling arenas

### Arena groups on one server

`arena_capacity` splits a busy server into independent arenas of that many human players, all on the same process, port and relay. Players join the fullest arena that still has room, or the lobby they picked if it has room; a new arena opens only when every arena is full, up to `ceil(max_players / arena_capacity)` arenas (at most 16). Each arena has its own match, bots, mods, round timer and spectators; voice, effects, kill feed and player join/leave notices stay within the arena, while chat rooms, roles, bans and announcements remain server-wide. Entity IDs are unique across arenas, so `kick`, `ban` and `role` keep working by entity ID. A quiet trailing arena closes after a minute without players. When a server-wide round starts (map rotation, `restart`, a settings change), players are regrouped into as few arenas as their groups allow, never splitting the players of one arena. `status` lists every arena; `players` shows each player's arena; clients see `· Arena n` after the server name.

The central profile uses `arena_capacity = 20` with `max_players = 80`, so up to four groups of twenty. Servers default to `arena_capacity = 20`. Set `arena_capacity = 0` to keep one arena of any size; a capacity above `max_players` is one arena of `max_players`. Party servers always use one arena.

`adaptive_bots = true` is the default for official and community servers. `bot_count` is a ceiling: the arena fills to six participants, with zero bots at six human players. Co-op wave enemies are separate. With no humans connected, the simulation and round timers pause while networking continues to accept joins. Use `adaptive_bots = false` or `--fixed-bots` for a fixed bot population.

Realtime input bursts from delayed connections are coalesced. The server retains bounded processing and drops excess input instead of kicking a player for packet bunching. Malformed input and control-message limits still apply. UDP buffers absorb short bursts where the operating system allows it; WebSocket queues are bounded and pause UDP reads when TCP is blocked.

Occupied arenas step concurrently on up to four available CPU cores; one active arena stays on the calling thread without allocating a work list. The game thread still orders network replication and round transitions after simulation completes. For more isolation, run arenas in separate server processes or on different machines. The central lobby should send players to an arena's UDP address and issue its connection token before they join. Account, database, and matchmaking requests belong outside the simulation thread; account verifiers should read cached claims and revocations.

Start with the current 64-player room limit and measure on the hosting hardware before increasing it. At 60 ticks per second, each tick has 16.7 ms; snapshot batches and network work need CPU time too. Monitor `status` for tick percentiles, snapshot time, skipped snapshots, and dropped catch-up ticks. Add rooms while there is still CPU and network headroom. The configured player limit alone does not establish capacity.

`configs/tankyard@.service` runs named arenas under systemd. For example, `tankyard@ffa-1` reads `/var/lib/tankyard/ffa-1.toml`. Give each arena its own `server_id`, bind port, ban, access and audit files. Set `public_addresses` and token issuance to match that arena. Set up the `tankyard` user and `/opt/tankyard` as for the [single service](#run-it-as-a-service), install the template as `/etc/systemd/system/tankyard@.service`, write each arena's settings, and start them with `sudo systemctl enable --now tankyard@ffa-1 tankyard@maze-1`.

Repeat these local workloads after server changes:

```sh
./target/release/tankyard-load --local --clients 64 --bots 24 --fixed-bots --seconds 30
./target/release/tankyard-load --local --clients 128 --bots 24 --fixed-bots --seconds 30
./target/release/tankyard-load --local --clients 8 --seconds 30 --latency-ms 100 --jitter-ms 20 --loss-percent 5
```

The load process also simulates every client, so its CPU use and loopback packet loss differ from a deployed fleet. Test the intended map, classes, and projectile population on the actual host. The central lobby and provisioning service remain separate integrations.

Creator badges use a per-connection challenge and Ed25519 proof checked against the public creator key. Display names and community roles do not confer creator identity. Standard builds have no owner-extension handler. Trusted hosts can register their own extension before accepting players; every extension packet carries a fresh signature bound to the connection nonce, current round, monotonic sequence, and exact payload. Bounded payloads, replay checks, rate limits, and audit logging apply before host code runs. Modified matches disable regular arena achievements.

## Relay hardening and shared addresses

Keep the Cloudflare route on WSS and leave the game UDP port unpublished when only the tunnel is needed. Run the server/relay as an unprivileged container user, preserve configuration and audit storage in writable volumes, and retain the existing tunnel credentials outside release packages. Guest transport remains supported; a game password does not establish a verified account. Secure netcode/account integration above is needed for authenticated persistent identity.

The relay bounds connections to 1–1024 (default 256), the complete WebSocket upgrade to five seconds, datagrams to 2048 bytes, queued writes to 64 KiB, and each direction to 2048 datagrams/second with a 4096-packet burst. Either direction going silent for fifteen seconds releases the slot; text and ping traffic do not count as game activity. Existing UDP/WebSocket framing, backpressure, voice, and guest joining are preserved. Cloudflare IP headers are parsed as diagnostic IP addresses only when the direct TCP peer is loopback; they never grant identity or authority.

Loopback bans target the connection ID and verified account, so banning one proxied guest cannot ban every player on the relay. Direct nonshared IP bans still cover that address. For a relay in another Docker container or host, add its exact source IP to the server's top-level `shared_relay_ips = ["172.18.0.2"]` before `[world]`. No client-supplied forwarding header is trusted for bans. Anonymous guests can change connection IDs when reconnecting; lasting individual bans behind a shared relay require verified accounts. Legacy address-wide bans on a configured shared relay are ignored rather than excluding all players.
