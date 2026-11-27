# Foley sources

Tank Yard's combat and map cues layer these recordings with pitch changes,
equalization, short crossfades, room reflections and peak control. The mixes are
built once and cached; playback does not synthesize or decode effects.

The source recordings are released under **CC0 1.0**:

- **rubberduck:** [80 creature SFX](https://opengameart.org/content/80-cc0-creature-sfx),
  [100 metal and wood SFX](https://opengameart.org/content/100-cc0-metal-and-wood-sfx),
  [40 water, splash and slime SFX](https://opengameart.org/content/40-cc0-water-splash-slime-sfx),
  [75 breaking, falling and hit SFX](https://opengameart.org/content/75-cc0-breaking-falling-hit-sfx).
- **bart:** [Steam release sounds](https://opengameart.org/node/5243) and
  [Steam whistle](https://opengameart.org/content/steam-whistle).
- **Brian MacIntosh / BMacZero:** [Metal Impact Sounds](https://opengameart.org/content/metal-impact-sounds).

- **Nocturnal_Vanguard (AuraVoice)** performed the Jester's entrance laugh. [Maniacal Laughter Pack 1](https://opengameart.org/content/maniacal-laughter-pack-1).

The full license is in [Foley-CC0.txt](../../../licenses/Foley-CC0.txt).
[sources.json](sources.json) records the exact source files, checksums and
processing. The shipped files are mono, 48 kHz, signed 16-bit little-endian PCM.
Only the recordings used by the game are included.

Maintainers can check the files with `python3 scripts/prepare_foley.py`.
`--rebuild` downloads the checksum-pinned originals and converts them with FFmpeg.
The sound designs are in `boss_sounds.rs` and `recorded_sounds.rs`.
To export the finished cues for listening, run this from the repository root:

```sh
TANKYARD_SOUND_DUMP="$PWD/target/sounds" cargo test -p tankyard-client --bin tankyard write_sounds -- --ignored
```
