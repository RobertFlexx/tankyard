# Parties

Open **Parties**, choose a mode, then create the party. The match runs on the leader's computer. Open the tank icons or press Enter to see members, mute incoming party voice locally, or use party text chat.

With **Internet setup: Automatic**, wait for **Internet relay ready** before copying an internet invite. Paste the entire `tankyard://party/...` link into Join a party. The link includes the leader's IP endpoint and an optional relay route; its invitation grants entry, never host or creator tools. Treat it as a private invitation.

Both sides open outgoing TLS WebSockets to `tankyard.kokonico.me`. The relay forwards game packets to the leader's local match, so neither player needs an inbound router rule. The central operator must install the v0.8.0 relay and preserve the `/party/host/*` and `/party/join/*` paths through Cloudflare. No private signing credentials belong on that server. Ordinary server joins still use the existing root WebSocket endpoint.

If the relay is unavailable, automatic setup also tries a temporary UDP router mapping. A detected public IP by itself does not establish reachability. Follow the invite screen's forwarding instructions, allow the game through the host firewall, and forward **UDP**, not just TCP. Double NAT may require forwarding at both routers; CGNAT usually needs a relay or a public address from the ISP. Prefer a relay if you cannot change the router.

For local play or an existing manual forward, choose **Manual / LAN** before creating the party and enter the desired IP:PORT. This disables public-IP requests, router discovery and relay registration. Editing the endpoint of an active invite selects that direct address.

Use **Return to menu** to keep the party. Open **Party modes**, choose the next setup, and click **Start party round**. Everyone stays connected; guests in menus can resume when ready. Sandbox is available to the party leader, and friends replace friendly bot slots automatically. **Leave party** ends the leader's hosted match and requires confirmation in the party panel.

Automatic address discovery uses fixed HTTPS services, and router mapping requests only a leased UDP entry that the game verifies and renews. Ending the party deletes only its own entry; a crash lets the lease expire. The game does not change your firewall. The preferred local port is 5000; an occupied port is replaced by an available port shown in the invite screen. The parser accepts IPv4 and bracketed IPv6 endpoints; the built-in host listens on IPv4.

Party voice mutes follow the member's connection across respawns and rounds. Enter while alive or Slash opens party chat; new messages appear in the collapsed roster. The online match keeps running while you browse menus.

The host role comes from a process-local channel. The invitation proves entry with a fresh signature bound to the connection, server and administration key. It stays out of saved server history and is never sent as a password. Relayed traffic uses TLS between each participant and the relay; the relay can observe forwarded game traffic. Direct UDP parties retain the regular unsecure netcode transport, so invitation proofs alone do not authenticate the server or encrypt chat/voice against an on-path observer. Creator and central-owner credentials remain separate.
