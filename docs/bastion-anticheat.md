# Bastion Anticheat

The Bastion keeps watch over fair play. Its small Credits mark uses the same Bastion model as the arena boss.

`crates/bastion-anticheat` is the Rust validation boundary used by the regular server. Clients send bounded intentions: movement, aim, fire, and ability buttons. The authoritative simulation owns elapsed time, position, collision, health, damage, weapon cooldowns, skills, upgrades, and spawn protection. Sending more input or a larger sequence number cannot buy more simulation ticks, faster weapons, or movement beyond the server's rules.

Finite input validation rejects impossible values and normalizes diagonal movement. Wrapping sequence comparison discards duplicate and reordered inputs. Large forward sequence gaps are allowed, including after a client pauses or loses traffic. Redundant input bundles, normal simulation deadlines, bounded packet processing, and rate limits remain in place. Packet floods can be dropped; they never increase the simulation rate.

There are no suspicion scores, ping thresholds, client fingerprint checks, or automatic cheating bans. Lag, packet loss, repeated inputs, and differing render frame rates are not evidence of cheating. A malformed protocol packet can still close its connection under existing transport rules, and an idle/disconnected player can time out normally. Those outcomes do not create a cheat accusation or a persistent ban. Server operators retain explicit moderation tools.

## Creator authorization

Bastion verifies the pinned Ed25519 creator identity and each exact signed administration request. Proofs bind a fresh random nonce, connection, and server identity. Commands additionally bind the current round, monotonic command sequence, and exact payload. Host policy, resource bounds, command rate limits, and request-before-execution auditing remain mandatory. A name, badge, moderator role, modified client, captured proof, or attacker-generated signing key does not grant creator powers.

Signed creator tools deliberately change authoritative rules. God mode, enhanced movement and weapons, unlimited abilities, boss/bot spawning, and safe teleports are permitted creator actions; Bastion does not flag or ban the creator for using them. This permission comes from the verified signing key, not from a client-controlled exemption bit. Normal bounded input still describes intentions; creator commands describe the authorized rule changes. Private servers and nonconsenting community hosts retain their own policy boundary. See [creator administration](creator-administration.md).

Cryptography proves possession of the private key, not the identity of an executable file. A stolen key could authorize another executable. Keep the private creator client and its credential private; signing secrets and creator-client code are excluded from the public repository and release packages. A leak requires revocation and key rotation. Executable hashes or client-reported "genuine binary" flags would not solve this trust problem.

## Scope and checks

This layer enforces server rules and command authenticity. It does not claim to detect every aimbot, wallhack, modified host, compromised computer, or future exploit. Existing per-viewer snapshots, visibility checks, bounded protocol decoding, relay limits, and deployment transport security remain separate protections.

Tests exercise nonfinite and oversized intentions, diagonal speed, sequence wrap/reordering/resume, server tick and weapon timing after large sequence gaps, foreign signing keys, replay and tampering, host revocation, and audit failures. Separate private integration checks use the real creator key without publishing it, including authorized powers during live network input.
