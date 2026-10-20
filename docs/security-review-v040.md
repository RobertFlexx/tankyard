# v0.4.0 security and network review

Reviewed the client privilege surfaces, server role/creator handlers, configuration policy, bounded protocol decoding, input sequence handling, relay framing/budgets, account sanctions and local storage. This is a scoped code/dependency review with regression tests, not a guarantee that every vulnerability has been found.

## Changes

- Give hosts, moderators and the central owner connection-bound signed administration requests. Central-owner proof binds its own pinned identity key to the session key. Separate signature domains prevent a creator/owner proof from becoming a different kind of command. Replays, tampering, wrong keys, rounds, servers and nonces fail verification. Local Host grants also check the client's public session key over the private process channel.
- Keep central ownership restricted to the central profile. Recheck role hierarchy, host consent, permissions, resource bounds and audit requirements. Neither privileged client has a game-wide ban packet, arbitrary code execution, shell, remote policy enablement or filesystem command.
- Backpressure reliable control bursts in a one-slot pending buffer plus the bounded transport queue instead of disconnecting a player because delayed packets arrived together. Realtime and voice work/memory limits, sequence checks, decompression caps, handshakes and relay deadlines remain enforced. Rate abuse does not create automatic cheat bans.
- Roll back failed ban storage so an error does not leave a hidden in-memory ban. Shared relay addresses still cannot receive a blanket player ban.
- Accept empty server IDs in nonprivileged community handshakes; explicit public creator opt-in still needs an ID and audit path. Normal dedicated defaults can join without an invalid creator-challenge error.
- Remove Postcard's unused default embedded-target collection feature, eliminating its deprecated atomic-polyfill dependency.

## Evidence and remaining limits

The full public Rust suite, installer fixtures, Clippy, real-key integration tests, native GUI checks and an impaired-link release workload are recorded in [status](status.md). The real-key tests exercise both pinned identities against ordinary servers, identity impostors, host consent, tampering/replay, revocation/re-enable, signed powers and server-local bans. Native checks use ordinary public clients with adjacent/profile key files and renamed self-contained editions from unrelated working directories.

RustSec's advisory database reports no vulnerability entries for the current lockfile. It does report the existing [Macroquad soundness advisory](https://rustsec.org/advisories/RUSTSEC-2025-0035.html), for which the audit lists no fixed version, and maintenance notices for [fxhash](https://rustsec.org/advisories/RUSTSEC-2025-0057.html) and [ttf-parser](https://rustsec.org/advisories/RUSTSEC-2026-0192.html). These graphical dependencies are outside the dedicated server's dependency tree. Main-thread graphics and explicit GPU teardown do not remove the upstream advisory; changing the rendering engine is separate work.

A valid key proves key possession, not the identity of an executable. A copied private executable/key grants its holder the same authority. Operators control their own server processes and may modify public source; clients cannot impose a universal trust policy on independently modified hosts. Achievements are local cosmetics, not remotely attested account awards.

Signatures authenticate staff commands but do not encrypt the entire game session. Use secure netcode tokens for authenticated UDP and verified WSS/TLS through Cloudflare for public relay access. Server-ID claims alone do not authenticate a remote server. Account verifiers must validate destination-bound claims and cached revocation without blocking the simulation thread.

Server bans are scoped to local state. Verified-account sanctions can identify a reconnecting account; direct address bans affect that address. Anonymous clients behind a shared relay have connection-scoped bans to avoid banning bystanders. A new anonymous identity may evade that connection ban; persistent, non-bypassable anonymous bans cannot be promised. This implementation adds no invasive hardware fingerprinting or cross-server blacklist.
