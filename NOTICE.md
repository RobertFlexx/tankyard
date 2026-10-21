# Third-party notices

Tank Yard uses original source and procedural geometry. Familiar tank names
are descriptive labels. No Diep.io code, logo, artwork or other game assets are
bundled. Tank Yard is independent and is not affiliated with Diep.io.

The client bundles the unmodified DejaVu Sans Bold font in
`assets/fonts/DejaVuSans-Bold.ttf`. Its copyright notices and full Bitstream Vera
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
