# v0.4.1 scoped review

Reviewed the new settings reset, map generation/scaling, creator map command, staff toggles, bounded decoding and custom-map validation. Existing signed staff authority, host policy, revocation, auditing and server-local moderation remain enforced.

- Reject excessive custom-map validation work before navigation traversals. A small wire payload could previously describe many large grids with dense wall sets and consume disproportionate CPU time. The catalog now has a combined geometry-work budget, alongside existing size and schema limits.
- Resolve intersecting walls before distant fallback candidates, including walls beyond the first 256 entries. Large maps no longer choose an unrelated distant wall as the first collision-recovery destination.
- Validate creator map selections against the loaded catalog. Prepare new geometry and safe entity positions before committing a change; restore geometry, objectives, configuration, IDs and RNG on failure. Existing identities and server authority are retained. The command still requires a current creator signature and consenting host policy.
- Preserve profile progress and saved data when resetting preferences. Key files remain outside the preferences object. Clear active menu capture when defaults are applied.
- Keep generated cover within the arena, reject unintended procedural overlap in regression checks, and derive animated wall geometry from its canonical layout to avoid coordinate drift.

Validation includes the public Rust suite, installer fixtures, Clippy, content/mod checks, real UI clicks and a signed creator map change on an ordinary central-profile server. See [status](status.md). This review does not resolve the upstream dependency and anonymous-ban limitations documented in the [v0.4.0 review](security-review-v040.md).
