# Bastion Anticheat

The server runs movement, collisions, weapons, upgrades and damage. Your client sends steering, aim and button presses. A faster frame rate or a burst of delayed packets gives a tank the same number of simulation steps as everyone else.

## What causes a disconnect

Bastion acts only on proof: a request the game's own client never sends, whatever the network does to its packets. Once a player has joined, it looks for four things:

- an input frame or packet outside the limits of the negotiated protocol;
- any other request outside those limits;
- two different inputs for the same frame;
- frames out of order inside one packet.

Input packets repeat up to three recent frames, so a lost packet need not cost a player a tap. The game numbers one frame per tick and never changes a frame once it has been sent, so every copy of a frame must match, and the frames in a packet always run from oldest to newest. The server checks the original values before normalizing diagonal movement or combining taps.

Every offending request is discarded whole. Each proof counts once, however many later packets repeat the same bad frame. Three distinct proofs within a minute close the connection, and the disconnect message gives the reason. Players can reconnect. A modified client that sends such requests produces several a second; one stray request can't end a session.

Identical retransmissions, packets arriving out of order, gaps after a pause and sequence wraparound are ordinary and never count. The frame history is kept for each connection and cleared for a new round; proofs carry over until they are a minute old. Aim accuracy, latency and frame rate play no part in the decision.

An unreadable packet is not a proof, since a newer release of the same protocol may send a request this server doesn't know. It is dropped, and the separate malformed-traffic limit closes connections after more than twelve such packets in ten seconds. Staff requests are checked against game data, such as the allowed rule ranges, that a patch release can change. One that fails is refused with a message and never counts as a proof.

## Host settings

Dedicated servers default to `bastion_mode = "enforce"`. Set `bastion_mode = "observe"` in the server TOML and restart to review findings while leaving players connected. Both modes discard the offending request and count its proof; only enforce mode closes the connection. Version handshakes and malformed-traffic floods keep their own disconnect rules.

Findings go to the server log and to the configured moderation audit file (`creator-audit.jsonl` on official hosts that leave `audit_file` unset). When enforcing, each distinct proof gets a line, such as "Recorded proof 1 of 3", and the disconnect gets the last. When observing, each kind of finding is written once per connection. If an audit write fails, the error is logged and the next finding of that kind is written instead of being skipped. Records identify the connection and rule. A Bastion disconnect creates no account or IP ban, including for people sharing a relay.

## Information hiding

Snapshots carry every tank for the leaderboard and minimap, but opponents more than the interest radius plus 400 units away are sent without aim, cursor distance, velocity, charge or weapon timers, and their positions are snapped to a 96-unit grid. Radar overlays and aim readers therefore get nothing that the minimap does not already show, and the snapshots are smaller. Teammates, the spectated tank and everything nearby keep full detail. Projectile hit histories, which only the server uses, are never sent.

## Creator authorization

Bastion verifies the pinned Ed25519 creator identity and each exact signed administration request. Proofs bind a fresh random nonce, connection, and server identity. Commands additionally bind the current round, monotonic command sequence, and exact payload. Host policy, resource bounds, command rate limits, and request-before-execution auditing remain mandatory. A name, badge, moderator role, modified client, captured proof, or attacker-generated signing key does not grant creator powers.

Signed creator tools deliberately change authoritative rules. God mode, enhanced movement and weapons, unlimited abilities, boss/bot spawning, and safe teleports are permitted creator actions; Bastion does not flag or ban the creator for using them. This permission comes from the verified signing key, not from a client-controlled exemption bit. Normal bounded input still describes intentions; creator commands describe the authorized rule changes. Private servers and nonconsenting community hosts retain their own policy boundary. See [creator administration](creator-administration.md).

Cryptography proves possession of the private key, not the identity of an executable file. A stolen key could authorize another executable. Keep staff credential packs and identity seeds private. Private signing credentials are excluded from the public repository and release packages. A leak requires revocation and key rotation. Executable hashes or client-reported "genuine binary" flags would not solve this trust problem.

## Scope and checks

This layer enforces server rules and command authenticity. It does not claim to detect every aimbot, wallhack, modified host, compromised computer, or future exploit. Existing per-viewer snapshots, visibility checks, bounded protocol decoding, relay limits, and deployment transport security remain separate protections.

Tests replay the game's own input stream through loss, duplication, delays of up to three seconds and shuffled delivery, across round restarts and sequence wraparound, and require that it never produces a finding. A real client on an emulated poor connection must finish without one too. Other tests cover nonfinite and oversized intentions, diagonal speed, server tick and weapon timing after large sequence gaps, the three-proof limit and its window, repeated proofs, foreign signing keys, replay and tampering, host revocation, and audit failures.
