# Server administration

Any current game client can use creator or central-owner tools when it holds the corresponding private signing key. Menu and verification code are public and reviewable. Public binaries and GitHub contain no private signing credentials. The optional self-contained credential editions remain private.

Version 0.4.1 adds a bottom-right owner toggle with authorization diagnostics and a Maps tab in the creator panel. Map changes are signed, audited and checked against the server's catalog. They retain identities, roles, upgrades and scores, refresh map-dependent objects and projectiles, and roll back if current entities cannot fit safely.

Version 0.5.0 expands the console and adds **Showtime → Become Axiom**. Axiom, the Worldshaper has an original model, 120,000 health and a shared top-screen health bar. Its 180-unit body radius exceeds every regular boss. Collapse the console to move and use twelve attacks in the grouped panel: Starfall, Sunburst, Comet Lance and Meteor Shower; Constellation, Repulse, Gravity Well and Rift Dash; Nova Mines, Star Lattice, Solar Turrets and Reforge. Hover an attack to see its effect. Rift Dash stops at walls and borders; Reforge heals eight percent with a thirty-second cooldown. The server enforces cooldowns, projectile limits and protected-player damage rules. Returning to the tank or resetting restores class stats; revocation also removes the form's shots. Death and a new round remove the form. This is a modified event round and does not record normal arena progress. The central-owner role has no access to this form.

## Roles and scope

| Identity | Authorization | Available tools |
| --- | --- | --- |
| Server Host | Local hosting channel or explicit process-console grant | Server-local moderation, roles, rooms, bots, match settings, restart and audit |
| Moderator / Admin | A grant from this server's owner | Only the selected server permissions |
| Central Server Owner | Separate pinned Ed25519 key, verified on the official central profile | Central-server moderation, announcements, roles, match settings, bots and audit; no creator gameplay modifiers |
| Game Creator | Pinned creator Ed25519 key and consenting host policy | Creator gameplay controls plus server-local kick, ban, mute, warnings and announcements |

Badges are assigned by the server, carried through snapshots, respawns, chat and elimination records, and shown beside names. Names and client labels never confer authority. The central-owner meeting achievement requires another human with the verified badge; it is a local cosmetic achievement, like the creator meeting achievement.

Bans affect only the current server and its local ban/account records. Neither client can ban someone from the whole game or from another dedicated server. A moderator cannot affect equal or higher server authority, or the verified creator. No game-wide moderation packet exists. An operator always controls their own process and configuration.

## Keys and portability

Use a matching **raw 32-byte Ed25519 seed** named `creator.key` or `central-owner.key`. The keys are different; a generated replacement or a public verification key cannot authenticate as either identity.

The public client checks beside its executable and then the `keys` directory beside its profile's `settings.json`. Settings → Profile shows that profile path. On Linux the default is `~/.config/tankyard/keys/`. Restart after installing a key. Lookup never depends on the working directory. A key in the profile works after moving or renaming the executable; an adjacent key travels with the executable. Optional private credential editions embed the matching key and work without sidecars.

Keep key files readable only by their owner (`chmod 600` on Unix). Keep an embedded-credential executable private too: sharing it shares its authority. No software-only check can establish that a particular executable, rather than another client holding the same key, signed a request. Compromise requires revoking the affected policy and rotating that identity's public/private key pair. Public source changes alone cannot manufacture a matching signature.

F8 opens or collapses creator tools when a creator key is loaded; otherwise it opens the server moderation menu when this connection has permissions. The Server button also opens moderation. Escape closes the panel. Warnings remain for twelve seconds or until dismissed; announcements last five seconds and leave an unread warning visible. Duration sliders control temporary mutes and bans. Creator gameplay powers are authorized under [Bastion Anticheat](bastion-anticheat.md).

## Hosting policy

Personal **Host server** games assign their local client the Owner role over a private process channel, binding the grant to its connection ID and public session key. There is no first-join or username-based owner claim. For dedicated community servers, use the process console's `players` and `owner-client <client-id> [public-key]`, or `role <entity-id> owner`, to explicitly designate a connected owner. Use secure connection tokens or WSS for remote staff access.

Official password-free servers enable creator tools by default. `creator_admin = false` overrides that policy. Community servers require explicit opt-in:

```toml
creator_admin = true
server_id = "my-public-arena"
audit_file = "configs/creator-audit.jsonl"
```

Private/password-protected hosts cannot enable creator administration. Public reachability is not guessed from addresses. No client packet, ordinary Owner role, hostname or URL can enable host policy.

The separate central-owner identity is accepted only with `kind = "Official"`, `server_id = "tankyard-central"`, and no password. That profile enables it by default; `central_owner_admin = false` explicitly revokes it. Other official and community servers do not grant central-owner jurisdiction.

Console commands `creator-admin off` and `central-owner off` revoke their corresponding access. Re-enabling rotates challenges and requires new proof; old signatures cannot recover access. Revocation remains effective if auditing fails. `save <config-path>` persists policy changes. Creator revocation removes active god mode, ability and movement/weapon modifiers and bot freezing; already-created world changes last until reversed or the round restarts.

## Verification and limits

Creator and central-owner proof domains are separate. Fresh random 256-bit nonces bind identity to the server and connection; central-owner proofs also bind the ephemeral administration public key. Every staff request is signed over its domain, server ID, nonce, connection, round, monotonic sequence, payload length and exact payload. The server rechecks signatures, policy, permissions, hierarchy, budgets and bounds. Ordinary Owner grants cannot invoke creator gameplay powers.

Creator and central-owner access require writable request auditing before privileged execution. Official servers without an explicit audit path use `creator-audit.jsonl`; Docker should mount it persistently. Logs rotate after 8 MiB with private Unix permissions. Host moderation uses the configured audit path. Requests are rate-limited and contain bounded data, never executable code. See the [security review](security-review-v040.md) for remaining transport, dependency and anonymous-ban limits.

## Central Pi deployment

See [the Pi/Docker upgrade steps](central-pi-upgrade.md). Publishing a release does not replace the running server. Version 0.5.0 uses protocol **17**: clients, server and relay must update together. No creator-server binary or server-side private identity key is needed.
