# Upgrade the central Pi

The central server runs in Docker behind the existing Cloudflare tunnel. These steps are for the machine's operator; this workspace has no live deployment access. Release v0.8.1 uses protocol **20**, unchanged from v0.8.0. The party disconnect fix requires updating the running `tankyard-relay`. Existing v0.8.0 clients remain compatible; clients and servers on older protocols must upgrade together.

1. Record the current Compose service/container names, image, commands, volume mounts, user, network and tunnel route. Back up the current mounted configuration, audit and ban/account data. Keep the old image or binary mount available for rollback.
2. Run `uname -m` **inside the target container** and check its libc. An `aarch64` glibc-based image uses `tankyard-v0.8.1-linux-aarch64.tar.gz`. An `x86_64` glibc-based image uses the x86-64 archive. These packages are not for 32-bit ARM or Alpine/musl images; use an appropriate glibc base image or build for that deployment.
3. Download the matching archive and `SHA256SUMS` from the [v0.8.1 release](https://github.com/RobertFlexx/tankyard/releases/tag/v0.8.1). Verify its SHA-256 against that file before extracting. Install the new regular `tankyard-server` **and** `tankyard-relay` into the deployment's existing image or binary mount. Retain executable permissions. If the Dockerfile copies binaries, rebuild that existing image with the new files; pulling an unrelated image does not update it.
4. Preserve the existing world settings, ports, credentials, mounts and Cloudflare routing. The central configuration should contain the following top-level fields; use paths inside the existing writable persistent mount:

   ```toml
   kind = "Official"
   server_id = "tankyard-central"
   audit_file = "/your/persistent/mount/creator-audit.jsonl"
   ban_file = "/your/persistent/mount/bans.json"
   # creator_admin = false       # explicit operator revocation, if desired
   # central_owner_admin = false # separate explicit revocation
   ```

   The two signed identities are enabled by default on this password-free central profile. Preserve any explicit false override. Do not copy creator or central-owner private keys into the Pi/container.

   The exact server ID matters. A Docker-generated ID such as `tankyard-docker` does not enable central ownership. Change the mounted configuration or its generator; `--central` preserves an existing nonempty ID.
5. Start the regular server using `tankyard-server --central --config <existing-mounted-config>`, or `--central` for the shipped profile. Keep the existing relay destination, usually `127.0.0.1:5000` within the same container, or the existing Compose service address across containers. Keep the Cloudflare tunnel pointed at the existing relay/WebSocket port. Forward `/party/host/*` and `/party/join/*` paths to this same relay without stripping the path; the usual whole-hostname route already does this. Use the deployment's normal Compose recreate/restart procedure for the actual service names.
6. Check both binaries report 0.8.1, inspect startup logs and join with a regular 0.8.1 client through `tankyard.kokonico.me`. Then join with each matching key: confirm the creator/central-owner badge, one harmless announcement, and its audit record. Test `creator-admin off` and `central-owner off` from the local console, then deliberately re-enable if desired. With `--no-console`, change the corresponding configuration flag and restart instead.
7. Create a party with Automatic internet setup and wait for **Internet relay ready**. Copy a fresh invite and join from a second network without adding a router forward. Verify party chat, then change modes from **Party modes** in the main menu. The leader still runs the simulation; the Pi only forwards packets. If the relay is unavailable, direct IP invites still require a working UDP forward and host firewall exception.
8. If a health check fails, restore the previous image/binary mount and configuration backup together. Use clients matching that previous protocol while diagnosing. Do not delete persistent ban or audit storage as a troubleshooting step.

The optional private clients belong on their owners' computers. Neither key is required by the server; it verifies public keys included in the regular server build.
