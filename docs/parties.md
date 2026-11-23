# Parties

Open **Parties**, choose a mode, then create the party. The match runs on the leader's computer. Open the tank icons or press Enter to see members, mute incoming party voice locally, or use party text chat.

With **Internet setup: Automatic**, wait for **Internet relay ready** before copying an internet invite. Paste the entire `tankyard://party/...` link into Join a party. The link includes the leader's IP endpoint and an optional relay route; its invitation grants entry, never host or creator tools. Treat it as a private invitation.

Both sides open outgoing TLS WebSockets to `tankyard.kokonico.me`. The relay forwards game packets to the leader's local match, so neither player needs an inbound router rule. The central operator must install the v0.8.0 relay and preserve the `/party/host/*` and `/party/join/*` paths through Cloudflare. No private signing credentials belong on that server. Ordinary server joins still use the existing root WebSocket endpoint.

If the relay is unavailable, automatic setup also tries a temporary UDP router mapping. A detected public IP by itself does not establish reachability. Follow the invite screen's forwarding instructions, allow the game through the host firewall, and forward **UDP**, not just TCP. Double NAT may require forwarding at both routers; CGNAT usually needs a relay or a public address from the ISP. Prefer a relay if you cannot change the router.

For local play or an existing manual forward, choose **Manual / LAN** before creating the party and enter the desired IP:PORT. This disables public-IP requests, router discovery and relay registration. Editing the endpoint of an active invite selects that direct address.

Use **Return to menu** to keep the party. Open **Party modes**, choose the next setup, and click **Start party round**. Everyone stays connected; guests in menus can resume when ready. Sandbox is available to the party leader, and friends replace friendly bot slots automatically. **Leave party** ends the leader's hosted match and requires confirmation in the party panel.

**Change map…** opens the map picker, on the Parties page before you create the party and under **Party modes** while it runs. It lists the built-in arenas, **My maps** from the Arena Workshop, and your **Map packs**. Friends don't need the map installed: they receive it when the round starts, and so does anyone who joins later. If a map or pack won't load, the current round carries on and the leader sees why. A Boss Rush party fights the leader's [lineup](playing.md#boss-rush).

Automatic address discovery uses fixed HTTPS services, and router mapping requests only a leased UDP entry that the game verifies and renews. Ending the party deletes only its own entry; a crash lets the lease expire. The game does not change your firewall. The preferred local port is 5000; an occupied port is replaced by an available port shown in the invite screen. The parser accepts IPv4 and bracketed IPv6 endpoints; the built-in host listens on IPv4.

Enter while alive or Slash opens party chat; new messages appear in the collapsed roster. The online match keeps running while you browse menus.

The host role comes from a process-local channel. The invitation proves entry with a fresh signature bound to the connection, server and administration key. It stays out of saved server history and is never sent as a password. Relayed traffic uses TLS between each participant and the relay; the relay can observe forwarded game traffic. Direct UDP parties retain the regular unsecure netcode transport, so invitation proofs alone do not authenticate the server or encrypt chat/voice against an on-path observer. Creator and central-owner credentials remain separate.

## In a match

The roster on your screen shows each member's tank, with a health bar that turns red when they're in trouble. A member who is down goes grey until they respawn. When someone joins or leaves, a short line appears beside the roster.

On a team, arrows at the edge of the screen point toward allies who are out of view, with their names. They're for allies only: opponents in your party still show up just on the minimap.

The party panel lists each member's level, what they're doing, their tank and their health. The leader can remove a member with **Kick**, then **Sure?**. Nobody can kick the leader.

## Voice

Everyone in a party hears each other at full volume, wherever they are: across the arena, while down, in a menu or between rounds. Your microphone starts muted and stays as you set it from round to round in the same party.

- **M** or the microphone button mutes and unmutes you. It's the same button as in a match, and it's also on the party roster at the side of the screen and on your own row in the party panel.
- While your microphone is on behind a menu, a small marker at the corner of the screen says so.
- A ring around a member's tank icon means they're talking. The button on their row mutes them for you only, and that mute follows them across respawns and rounds.
- A crossed-out microphone on a member's icon means their mic is off, so you can stop wondering why nobody answered. It shows for members on 2.0.3 or newer.
- A server with voice turned off carries no party voice.

The leader's game carries the voice, so the leader needs 2.0.2 or newer for this. A 2.0.0 leader still carries voice by distance, as in an ordinary match.

## Ready check

Open the party roster and click **I'm ready**. Every member sees the ready markers and count, alongside whether each person is in a menu or a match. Click again to clear your status. Readiness resets when a new round starts. It is a coordination aid: the current match keeps running and readiness never grants leader permissions.
