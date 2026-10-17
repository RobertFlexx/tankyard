# 0.2.13 source audit

This is an unreleased source candidate. It has not been compiled in this editing environment.

## Fixed or changed

| Area | Finding | Action |
| --- | --- | --- |
| Gameplay | A one-frame click could vanish before a charge weapon fired, and some built-in long-range guns needed a charge instead of normal click-to-fire | Built-in charge barrels now use ordinary cooldown firing with their former full-charge size and damage; core charge weapons supplied by mods also release after one tick |
| Input/HUD | A press accepted over the arena could be dropped if the mouse crossed a HUD control before the next 60 Hz tick | Once the press is accepted, UI hover changes cannot cancel the queued pulse; held clicks still respect HUD hitboxes |
| Gameplay | Piercing Sprayer's second barrel was not aligned with its primary spray | Aligned both forward and made the slower piercing shot a normal shot |
| HUD | Kills did not display the eliminated player's name | Authoritative death events carry the victim ID/name to a 3-entry, 3.5-second top-screen feed |
| Networking/UI | Two equal-reward kills on the same tick could be collapsed by event deduplication | Victim ID participates in the dedupe key; reliable event tests cover the metadata |
| Event pressure | A full 512-event buffer could drop kills, match results and level-up notices after shot spam | Important events displace a shot/impact event while preserving the hard cap |
| Security/QOL | A malformed elimination name from an untrusted server could contain control characters or flood the HUD | Rendered names are filtered and capped at 24 Unicode characters, and the feed is bounded |
| AI | Each new server round reset the trained online networks | Keep bot motion/build model weights across rounds but reset sightings, relationships and temporary combat feedback |

## Compatibility and deliberate limits

- Protocol version is 11. Update both the client and the authoritative server. This is not a network-compatible drop-in replacement for 0.2.12.
- The normal-fire change affects built-in class balance. All 102 behavioral fingerprints remain distinct in a static asset check, but actual damage curves and match balance need live gameplay testing. Some formerly charged weapons now deal their full charged damage each reload.
- The bot models are bounded online neural networks, not a persistent large-model service. Model weights carry between rounds in the same server process only. Bot difficulty still caps perception, reaction rates, and aim accuracy. No account or opponent-private data is persisted.
- Local data stores, browser clients, and servers that speak protocol 10 will not speak protocol 11.
- **Existing security boundary:** ordinary host/join still uses unauthenticated netcode intended for private/local games. Do not expose those sessions as a trusted public service without the separately described token/identity infrastructure. The changes here do not implement that infrastructure.
- This is a targeted source audit, not a complete penetration test, dependency vulnerability scan, or rendering verification.

## Release gates

From a machine with a Rust toolchain, native graphics libraries and UDP access:

```sh
scripts/check_source.sh
cargo run --locked -p tankyard-tools -- validate
cargo build --locked --release --workspace
scripts/smoke_client.sh --all
```

Manually verify mouse taps (including high-refresh monitors), reload bars, Piercing Sprayer, same-tick multi-kills, 800x600 HUD layout, joined clients receiving kill names, new-round AI model reuse, and connection refusal with an outdated protocol. Run the existing UDP/WebSocket load suites before tagging. Review DPS changes on every converted barrel, particularly homing, explosive and high-penetration weapons.

The checks in this file are instructions, not claims that they passed here.
