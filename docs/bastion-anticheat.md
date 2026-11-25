# Bastion Anticheat

The server runs movement, collisions, weapons, upgrades and damage. Your client sends steering, aim and button presses. A faster frame rate or a burst of delayed packets gives a tank the same number of simulation steps as everyone else.

## What causes a disconnect

Once a player has joined, Bastion checks each decoded request against the limits of the negotiated protocol. Input packets repeat up to three recent frames so a lost packet need not cost a player a tap. Two copies of the same recent frame must describe the same input. The server checks the original values before normalizing diagonal movement or combining taps. A contradiction discards the entire request and, by default, closes that connection. The disconnect message gives the reason. Players can reconnect.

Identical retransmissions, out-of-order delivery, gaps after a pause and sequence wraparound are ordinary. Bastion keeps a short history for each connection and clears it for a new round. Aim accuracy, latency and frame rate play no part in the decision. A single unreadable packet after joining is dropped. The separate malformed-traffic limit closes connections after more than twelve such packets in ten seconds.

## Host settings

Dedicated servers default to `bastion_mode = "enforce"`. Set `bastion_mode = "observe"` in the server TOML and restart to review findings from joined players while leaving those players connected. Both modes discard the offending request. Version handshakes and malformed-traffic floods keep their existing disconnect rules. Each kind of finding is reported once per connection in the server log and in the configured moderation audit file (`creator-audit.jsonl` on official hosts that leave `audit_file` unset). A failed audit write is logged and the finding is retried instead of being marked as reported. Records identify the connection and rule. A Bastion disconnect creates no account or IP ban, including for people sharing a relay.

## Information hiding

Snapshots carry every tank for the leaderboard and minimap, but opponents more than the interest radius plus 400 units away are sent without aim, cursor distance, velocity, charge or weapon timers, and their positions are snapped to a 96-unit grid. Radar overlays and aim readers therefore get nothing that the minimap does not already show, and the snapshots are smaller. Teammates, the spectated tank and everything nearby keep full detail. Projectile hit histories, which only the server uses, are never sent.

## Creator authorization

Bastion verifies the pinned Ed25519 creator identity and each exact signed administration request. Proofs bind a fresh random nonce, connection, and server identity. Commands additionally bind the current round, monotonic command sequence, and exact payload. Host policy, resource bounds, command rate limits, and request-before-execution auditing remain mandatory. A name, badge, moderator role, modified client, captured proof, or attacker-generated signing key does not grant creator powers.

Signed creator tools deliberately change authoritative rules. God mode, enhanced movement and weapons, unlimited abilities, boss/bot spawning, and safe teleports are permitted creator actions; Bastion does not flag or ban the creator for using them. This permission comes from the verified signing key, not from a client-controlled exemption bit. Normal bounded input still describes intentions; creator commands describe the authorized rule changes. Private servers and nonconsenting community hosts retain their own policy boundary. See [creator administration](creator-administration.md).

Cryptography proves possession of the private key, not the identity of an executable file. A stolen key could authorize another executable. Keep staff credential packs and identity seeds private. Private signing credentials are excluded from the public repository and release packages. A leak requires revocation and key rotation. Executable hashes or client-reported "genuine binary" flags would not solve this trust problem.

## Scope and checks

This layer enforces server rules and command authenticity. It does not claim to detect every aimbot, wallhack, modified host, compromised computer, or future exploit. Existing per-viewer snapshots, visibility checks, bounded protocol decoding, relay limits, and deployment transport security remain separate protections.

Tests exercise nonfinite and oversized intentions, diagonal speed, sequence wrap/reordering/resume, server tick and weapon timing after large sequence gaps, foreign signing keys, replay and tampering, host revocation, and audit failures.
