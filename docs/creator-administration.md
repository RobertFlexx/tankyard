# Creator administration

The regular v0.3.2 `tankyard-server` supports the separate creator client. A creator-server executable is no longer necessary. The creator client, menu, signing implementation, and secret key are maintained privately and excluded from this repository and public release packages. Public server handlers and their authorization checks are deliberately reviewable.

## Host policy

Official public arenas (`kind = "Official"`) enable key-verified creator administration by default in the **regular server**. Use `tankyard-server --central` for the included central profile, even in a container without a configs directory. No separate creator server or additional creator opt-in is required. A top-level `creator_admin = false` always overrides the official default and is preserved with custom configurations.

Community servers, older community configurations, personal **Host server** games, and password-protected arenas have creator tools disabled by default. A password disables the automatic official policy too; explicitly enabling tools on a password-protected host is rejected. No client URL, claimed hostname, username, badge, ordinary Owner role, or remote packet can change this policy. Official status is chosen by the operator's local configuration, never inferred from where a client claims to connect.

The official/central templates include a stable server ID and persistent audit location. Without an explicit audit path, official servers use `creator-audit.jsonl` in their working directory; tools fail closed if that file cannot be written. Mount this storage persistently in Docker.

An independently operated public community arena may opt in with these top-level fields:

```toml
creator_admin = true
server_id = "my-public-arena"
audit_file = "configs/creator-audit.jsonl"
```

The ID must contain 1–64 ASCII letters, digits, underscores, or hyphens. A community opt-in requires no password and an explicit audit path. Password-protected matches cannot enable creator tools. Leave the flag disabled on personal/private hosts. Public reachability is not inferred from an IP address: the operator's explicit configuration is the consent boundary.

The process console reports `creator-admin status`. `creator-admin off` immediately removes invulnerability, unlimited abilities, movement/damage/reload modifiers, and bot freezing. It invalidates command authorization and updates connected clients. It still revokes powers when the audit disk is unavailable. `creator-admin on` validates the host policy, writes an audit entry, and requires fresh authorization. Use `save <configuration-path>` to persist a console change, or edit the configuration before restarting. Gameplay edits already made, such as a spawned boss or granted level, remain until changed or the round restarts.

## Authorization and boundaries

Creator identity uses Ed25519 and the published verification key in `tankyard-protocol/src/creator.rs`. Each connection receives a fresh random 256-bit challenge. Proofs bind the server ID, connection ID, and nonce. A badge alone is cosmetic on a host whose policy disables tools.

Every administration request carries a separate signature over a distinct domain, server ID, session nonce, connection ID, round, monotonically increasing sequence, payload length, and exact payload. The server rechecks host policy on every action and rejects replayed, stale, forged, or modified commands. Revocation rotates the challenge; old authorization cannot become valid again when a host re-enables the policy. Commands are limited to 1024 bytes and share a budget of two per second with a burst of six. Ordinary packet limits also apply.

Signed requests are recorded before execution, including actor, target, and command parameters. An unavailable request audit prevents execution. Results are recorded separately, including rejected signed command values. Logs rotate after 8 MiB and use private file permissions on Unix. Unauthenticated or invalid-signature traffic does not fill the audit file. The server contains the verification key only; it cannot sign as the creator. Hosts can revoke all creator powers with their own policy setting. A leaked creator signing key requires revoking access and rotating the public identity key in a new client/server build.

Signed creator powers are authorized under [Bastion Anticheat](bastion-anticheat.md); using them does not produce cheat accusations or automatic bans. Key possession is the identity boundary. An executable hash or client claim cannot prove that a particular binary holds the key. Keep the private credential private.

Commands cover local moderation (kick, bounded ban, mute/unmute) and bounded arena tools: boss spawning, up to 128 bots within the configured population cap, invulnerability, unlimited abilities, speed/reload up to 6×, damage up to 8×, level/class/build changes, healing, projectile/boss cleanup, bot freezing, safe teleport, opponent elimination, and modifier reset. Moderation cannot grant roles, change host policy, access private files, or act on another server. The protocol accepts no scripts, executable modules, shell commands, arbitrary file paths, or outbound control requests.

Gameplay tools mark the arena modified and notify players; regular arena achievements stop for that round. Pure moderation preserves ordinary achievement eligibility. The verified badge and cosmetic meeting achievement remain available on consenting and nonconsenting servers. Round restarts clear temporary gameplay modifiers. Disconnecting the last creator clears bot freezing.

Command signatures protect request authenticity, not transport confidentiality or authentication of the host. Keep the existing TLS/WebSocket route and follow the [transport hosting guidance](hosting.md) for public deployment. A host controls its own process and may modify its own server build; this mechanism grants no authority over a third party's private server or machine.

## Central server in Docker on a Raspberry Pi

The live process must be upgraded; publishing a GitHub release does not deploy it. Check the Pi's architecture first: the Linux ARM64 release package is for an `aarch64` system. Keep the current Docker configuration, persistent volumes, ports, Cloudflare tunnel, and WebSocket relay route.

1. Download the matching v0.3.2 package, verify it against the release's `SHA256SUMS`, and update the regular `tankyard-server` and `tankyard-relay` binaries in the container image or existing binary mount. Both must use protocol 15.
2. Preserve the operator's world settings, bans, and other configuration. Use `kind = "Official"`, choose a stable `server_id`, and configure a writable persistent `audit_file` outside the disposable container layer. Omit `creator_admin` to use the official default; an existing explicit false remains a deliberate override. The included `configs/central.toml` is a starting template; its relative audit and ban paths must resolve into the existing persistent configuration mount.
3. Start the ordinary server with `tankyard-server --central --config <mounted-config-path>`, or `tankyard-server --central` with the shipped profile. No creator-server binary is involved. Keep the existing relay and Cloudflare routing configuration. Restart or recreate through the deployment's normal Docker procedure.
4. Join with a regular v0.3.2 client to check connectivity, then join with the private creator client. Verify the badge, one bounded command, and its request/result audit records. Test `creator-admin off` from the local process console when enabled. With `--no-console`, edit the flag in the persistent configuration and restart to revoke instead.

The private client and key belong on the creator's computer. Neither is needed in the Pi's container. This repository contains no deployment connection to the live Pi.
