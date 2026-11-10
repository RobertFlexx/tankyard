# Third-party notices

Tank Yard uses original source and procedural geometry. Familiar tank names
are descriptive labels. No Diep.io code, logo, artwork or other game assets are
bundled. Tank Yard is independent and is not affiliated with Diep.io.

The client bundles the unmodified DejaVu Sans Bold and DejaVu Sans Mono fonts in
`assets/fonts/DejaVuSans-Bold.ttf` and `assets/fonts/DejaVuSansMono.ttf`. Their copyright notices and full Bitstream Vera
and Arev license terms are in [LICENSE-DejaVu.txt](assets/fonts/LICENSE-DejaVu.txt).
These terms also apply to the font bytes embedded in the executable.
Macroquad retains its own bundled fallback font and upstream notices.

Macroquad, Miniquad, Renet, renet_netcode, Serde, Postcard, Clap and the other
Cargo dependencies retain their licenses. The project's MIT/Apache-2.0 license
covers Tank Yard's source and does not replace dependency or font licenses.
Voice capture and playback use CPAL. The microphone and muted microphone icons
are from [Lucide](https://lucide.dev), under the ISC license. Their unmodified
SVG sources, rasterized HUD copies, and full license are in
`assets/icons/lucide`; release archives also include `licenses/Lucide.txt`.

Proximity voice uses Opus 1.6.1 through the `opus` and `opusic-sys` crates.
The bundled codec's full BSD notice is in [licenses/Opus.txt](licenses/Opus.txt)
and is included in release archives. The boss arrival alarm is original audio,
reproducible with `scripts/generate_boss_alarm.py`, covered by this project's license.
Boss models are original procedural geometry; behavior references are documented
in [bosses and learning](docs/bosses-and-learning.md).

Linux window controls use `libloading` under the ISC license; its notice is in
[licenses/libloading.txt](licenses/libloading.txt). X11 and Xcursor are loaded
from the desktop to use its cursor theme and native monitor geometry.

Desktop clipboard integration uses Arboard, with X11/Wayland and Windows helpers.
Their notices are included in [licenses/clipboard-dependencies.txt](licenses/clipboard-dependencies.txt).

Party networking uses Ureq and XML parsers under the MIT license option.
Their notices are included in [licenses/party-networking.txt](licenses/party-networking.txt).

Mod scripts run on Luau, built from vendored sources with Tank Yard's changes
and linked statically, through the `mlua` bindings. Mod archives are read with the `zip`, `flate2` and
`zlib-rs` crates. Their full notices (MIT and Zlib) are in
[licenses/lua-and-archives.txt](licenses/lua-and-archives.txt), which release
archives include.

On Linux x86-64 and Windows x64, the client links NVIDIA's NGX loader from the
[NVIDIA DLSS SDK](https://github.com/NVIDIA/DLSS) 310.9.16, fetched at build
time by `scripts/fetch_dlss_sdk.py` and not kept in this repository. On a
GeForce RTX GPU, the game downloads NVIDIA's DLSS runtime from that SDK release
when the player chooses DLSS, and checks its SHA-256 before use. Both are
NVIDIA's software under the NVIDIA RTX SDKs license, whose text is in the SDK
(`LICENSE.txt`). NVIDIA, GeForce RTX and DLSS are trademarks of NVIDIA
Corporation. Tank Yard's own DLSS integration (`crates/tankyard-client/native`
and `src/dlss`) is covered by this project's license; the libstdc++ bridge
there reimplements a few GCC runtime functions and contains no NVIDIA code.
