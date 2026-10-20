# Parties

Open **Parties** in the main menu. Choose **Configure match** for any offline mode, map and bot difficulty, then **Create party**. The bot count becomes the number of slots per team, between two and eight. In team modes everyone joining the party starts on the same team. Free For All and Maze retain their individual combat rules; Tag retains team conversions.

The leader hosts the normal authoritative server on UDP port 5000. The party screen detects a local address. For internet friends, replace **Reachable IP:PORT** with your public IP and forwarded UDP port, then choose **Copy invite link**. Your router and firewall must allow the connection; carrier-grade NAT may require a VPN or a reachable host. There is no central party relay or automatic port forwarding.

Friends paste the complete `tankyard://party/IP:PORT#invite` link into **Join a party**. The random invite expires when the hosting session closes. Share it privately. Any invited member can share it with another friend. IPv4 and bracketed IPv6 address forms are understood by the invite parser; the built-in host listens on IPv4.

Human teammates take allied bot slots. Dead and lower-scoring bots are replaced first. When someone leaves, an allied bot returns. Larger parties increase the opposing team size. Co-op PvE's raiders and bosses belong to the mode and are never removed to make room for teammates. Teammates regroup around different human players between fights. Offline Co-op PvE and Boss Rush default to three friendly bots when selected in the menu.

The local leader receives the **Server Host** badge and server moderation/match tools through the private hosting channel. An invite grants entry, not ownership. Use Server tools to change map, mode, difficulty or team size; round changes retain the connections and put party members together again. Open **Pause → Party invites** to copy another invite while playing. The roster shows up to eight people without covering other controls.

Entry uses a fresh server challenge and a signature bound to the connection, server and administration session key. The invite's signing seed is never sent as a password, persisted in recent servers, or given to the server. Direct UDP parties retain the regular unsecure netcode transport; invite proofs do not authenticate a server or provide a private voice channel against an on-path observer. Existing creator/central-owner keys and host authority remain separate.
