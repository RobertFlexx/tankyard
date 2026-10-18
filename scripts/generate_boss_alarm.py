#!/usr/bin/env python3
"""Reproduce Tank Yard's original 48 kHz boss arrival sound (MIT license)."""
import math
from pathlib import Path
import struct
import wave

RATE = 48_000


def generate():
    samples = []
    phase = 0.0
    for index in range(int(RATE * 2.6)):
        time = index / RATE
        pulse = time % 0.9
        envelope = min(pulse / 0.025, 1.0) * max(0.0, min((0.72 - pulse) / 0.15, 1.0))
        frequency = 560 + 420 * math.sin(min(pulse / 0.72, 1.0) * math.pi) ** 2
        phase += math.tau * frequency / RATE
        siren = (math.sin(phase) + 0.22 * math.sin(phase * 2) + 0.08 * math.sin(phase * 3)) * envelope
        impact = math.sin(math.tau * (75 * time + 22 * (1 - math.exp(-time * 4)))) * math.exp(-time * 3)
        fade = min(time / 0.015, 1.0) * max(0.0, min((2.6 - time) / 0.35, 1.0))
        samples.append((siren * 0.46 + impact * 0.25) * fade)
    # Short room reflections add depth without masking the warning rhythm.
    dry = samples[:]
    for delay, gain in [(0.071, 0.12), (0.127, 0.08), (0.203, 0.045)]:
        offset = int(delay * RATE)
        for index in range(offset, len(samples)):
            samples[index] += dry[index - offset] * gain
    path = Path(__file__).resolve().parents[1] / "assets/audio/boss-arrival.wav"
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(RATE)
        output.writeframes(b"".join(struct.pack("<h", round(max(-1, min(1, sample)) * 32767)) for sample in samples))


if __name__ == "__main__":
    generate()
