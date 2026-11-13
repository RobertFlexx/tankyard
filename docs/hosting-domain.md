# Host Tank Yard on your own domain

Players can join `play.example.com` without remembering a port. Tank Yard runs the match and its WebSocket listener in one process; Caddy supplies HTTPS. You can use a subdomain of any domain you own, or the domain itself. You do not need to buy or operate a top-level domain.

Use client and server releases with compatible protocols. Tank Yard 1.0 uses **protocol 26**, which cannot exchange snapshots with protocol 25 in 0.15 and 0.16. Upgrade older clients and servers together; see the [workshop guide](map-editor.md#files-and-deployment) for maps.

## First, make a local server work

Install the server as [Hosting](hosting.md#a-dedicated-server) describes, then write a settings file:

```sh
curl -fsSL https://github.com/RobertFlexx/tankyard/releases/latest/download/install.sh | sh -s -- --server
tankyard-server --write-config my-server.toml
```

Edit the top-level settings in `my-server.toml`, before `[world]`:

```toml
server_id = "my-community"
name = "My community arena"
bind = "127.0.0.1:5000"
websocket = "127.0.0.1:8080"
max_players = 32
arena_capacity = 16
bot_count = 8
adaptive_bots = true
snapshot_rate = 20
```

Keep the generated `[world]` section. On Windows, run `.\tankyard-server.exe` in the extracted release folder instead. In **Host server → Community server**, the **Domain listener** toggle enables the same loopback listener on TCP 8080.

Start it from the directory containing the config:

```sh
tankyard-server --config my-server.toml
```

On that machine, join `127.0.0.1:5000` to test UDP, then `ws://127.0.0.1:8080` to test the WebSocket listener. Both reach the same arenas. Leave the server open during the next steps. `--websocket 127.0.0.1:8080` also works as a command-line override; no separate `tankyard-relay` process is needed.

Guest connections work without an account service. A server password controls admission; it does not verify a player's identity. WSS encrypts the connection to the HTTPS endpoint. Persistent verified roles and account bans use the separate authenticated account integration in [Hosting](hosting.md#from-the-command-line).

## Option A: a public address and Caddy

This suits a VPS, a public IPv6 host, or a home connection with incoming port forwarding.

1. In your DNS provider's dashboard, add an **A** record named `play` pointing to your server's public IPv4 address. Use `@` instead for the domain itself. Use **AAAA** only if this machine also has working, publicly reachable IPv6. A stale AAAA record can make joins fail even when IPv4 works.
2. Start with **DNS only** if your DNS provider offers a proxy toggle. A domain name does not forward ports by itself.
3. Allow inbound **TCP 80 and 443** in the host firewall and the provider's firewall. At home, forward those two TCP ports to the host's reserved LAN address. Keep TCP 8080 and UDP 5000 private for a WSS-only deployment.
4. [Install Caddy](https://caddyserver.com/docs/install) for your operating system. Put this in its Caddyfile, replacing the example hostname:

```caddyfile
play.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

5. Validate and run the configuration. With the packaged Linux service:

```sh
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
sudo systemctl enable --now caddy
```

If Caddy has not started yet, start it before reloading. Outside a service installation, `caddy run --config Caddyfile` keeps it in the foreground.

Caddy obtains and renews the certificate once DNS points to the host and the validation ports are reachable. Its reverse proxy handles WebSocket upgrades automatically; no manual `Upgrade` header rules are needed. See Caddy's [automatic HTTPS requirements](https://caddyserver.com/docs/automatic-https) and [WebSocket proxy behavior](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy).

Players now enter **`play.example.com`** or **`wss://play.example.com`** in Join server. Keep the root path `/` routed to Tank Yard. Do not place a browser login, CAPTCHA or a web page in front of that route: the native game cannot complete browser-only challenges.

Caddy configuration reloads can end active WebSocket connections. For frequent configuration changes, review its documented `stream_close_delay` option and schedule changes when the arena is quiet.

### Also offer direct UDP

Direct UDP avoids TCP's head-of-line blocking when packets are lost. To offer it alongside WSS, change Tank Yard's `bind` to `0.0.0.0:5000`, allow and forward **UDP 5000**, and retain the loopback WebSocket setting. Players use `play.example.com:5000` for UDP and `play.example.com` for WSS.

The hostname used for direct UDP must resolve to the host, rather than an ordinary HTTP proxy. Cloudflare's standard HTTP proxy does not carry arbitrary game UDP; use a DNS-only name such as `udp.example.com` for that route. A bare hostname chooses WSS in Tank Yard; an explicit `host:port` chooses UDP unless prefixed with `ws://` or `wss://`.

## Option B: a Cloudflare Tunnel

This suits carrier-grade NAT or a home router where you cannot forward incoming ports. Your domain's DNS must be set up for Cloudflare, and `cloudflared` runs on the host or on a machine that can reach it.

1. Keep Tank Yard listening on `127.0.0.1:5000`, with `websocket = "127.0.0.1:8080"`.
2. Follow Cloudflare's [tunnel setup](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/) and create a published application route for `play.example.com` with service **HTTP**, URL **`localhost:8080`**.
3. Run the connector as a service. Keep the tunnel token or credentials file private, outside mod packs, source control and release archives.
4. Ensure WebSockets are enabled for the hostname. Cloudflare documents the setting in [WebSockets](https://developers.cloudflare.com/network/websockets/).
5. Join `play.example.com` from another internet connection. No incoming router port is needed for the tunnel route.

For a locally managed tunnel, the relevant ingress is:

```yaml
ingress:
  - hostname: play.example.com
    service: http://localhost:8080
  - service: http_status:404
```

This is only the ingress fragment: keep the tunnel ID and credentials settings from your connector's own configuration. See Cloudflare's [configuration-file guide](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/configuration-file/).

With Docker, `localhost` means the connector's container. Use the Tank Yard service name and its internal TCP 8080 instead, and bind the WebSocket listener to `0.0.0.0:8080` inside that private network. Do not publish that port to the internet. Configure the relay source IP in `shared_relay_ips` as described in [relay hardening](hosting.md#relay-hardening-and-shared-addresses).

## Verify the endpoint

- Check both DNS address families: `dig A play.example.com` and `dig AAAA play.example.com`. On Windows use `Resolve-DnsName play.example.com` in PowerShell. Remove records pointing to an old host.
- Join from a phone hotspot or another external network. Testing only inside your LAN can hide forwarding mistakes or fail because the router lacks NAT loopback.
- An ordinary browser/curl request to the game listener can return HTTP 400 because it is not a WebSocket upgrade. That alone is not a failed health check. Verify with Tank Yard's server list or an actual join.
- Inspect the game console and Caddy/connector logs. Certificate failures concern the TLS endpoint; connection refused on 8080 concerns the listener; a WebSocket upgrade followed by a game rejection concerns the game's version/password/policy.
- Save the hostname as a favorite. The Join page then probes status and shows the server name, population and lobbies.

For a small authorized join/load check against your endpoint:

```sh
./target/release/tankyard-load --server wss://play.example.com --clients 2 --seconds 15
```

Do this only on your own host and include `--password` if required. The load tool creates real players.

## Keep a community server running

Use the [systemd service](hosting.md#run-it-as-a-service) and the [capacity guidance](hosting.md#scaling-arenas) for an always-on Linux host. On other systems, use a service manager that restarts the process on failure. Keep the config, bans, audit log and mod packs in persistent storage; back them up before updates.

Start with small arenas, adaptive bots and 20 snapshots/second. Watch the console's `status` timings and dropped ticks before raising limits. On multicore hosts, occupied arenas can step concurrently; one arena still runs on one simulation thread. A public server directory and account service are separate integrations, so publish your join address and matching game version to your community yourself.

Workshop maps are ordinary packs. Export a ZIP, add it to `mods`, and run `reload-mods`; validation failures preserve the running match. Use `maps` and `map my_arena` to select it. **Server → Rules** applies live rule changes; the process console's `save` command retains the current profile in the server config.
