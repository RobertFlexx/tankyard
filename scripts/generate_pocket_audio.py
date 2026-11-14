#!/usr/bin/env python3
"""Build Bacteria's room tone and entry sting (48 kHz mono signed 16-bit PCM).

Usage: python3 scripts/generate_pocket_audio.py /path/to/fluorescent-hum.mp3
The recording is 'fluorescent 1.wav' by theonlycanvas, CC0:
https://freesound.org/people/theonlycanvas/sounds/234462/
The low drones, duct noise and noclip sting are original synthesis.
Requires ffmpeg. Outputs assets/audio/pocket-room.pcm and pocket-enter.pcm.
"""
import array
import math
from pathlib import Path
import random
import subprocess
import sys

RATE = 48000
ROOT = Path(__file__).resolve().parents[1]

def write(name, samples):
    pcm = array.array('h', (round(max(-0.95, min(0.95, v)) * 32767) for v in samples))
    if sys.byteorder != 'little':
        pcm.byteswap()
    (ROOT / 'assets/audio' / name).write_bytes(pcm.tobytes())

def main():
    source = Path(sys.argv[1])
    raw = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(source), '-f', 'f32le', '-ac', '1', '-ar', str(RATE), '-'])
    field = array.array('f'); field.frombytes(raw)
    if sys.byteorder != 'little':
        field.byteswap()
    mean = sum(field) / len(field)
    peak = max(abs(v - mean) for v in field)
    rng = random.Random(0x4241435445524941)
    room = []
    air = 0.
    for i in range(RATE * 24):
        t = i / RATE
        air += 0.002 * (rng.uniform(-1, 1) - air)
        hum = (field[i % len(field)] - mean) / max(peak, 1e-6) * 0.28
        swell = 0.5 + 0.5 * math.sin(t * math.tau / 17)
        drone = math.sin(t * math.tau * 73.4) * 0.012 + math.sin(t * math.tau * 110.2) * 0.009
        room.append(hum + air * 0.20 + drone * swell)
    # The loop seam is a slow crossfade, never a discontinuity in the light hum.
    seam = RATE
    joined = [room[-seam + i] * (1 - i / seam) + room[i] * (i / seam) for i in range(seam)]
    room = joined + room[seam:-seam]
    write('pocket-room.pcm', room)
    sting = []
    filtered = 0.
    for i in range(int(RATE * 0.65)):
        t = i / RATE
        filtered += 0.18 * (rng.uniform(-1, 1) - filtered)
        attack = min(t / 0.009, 1.)
        impact = math.sin(math.tau * (102 * t - 54 * t * t)) * math.exp(-12 * t)
        tearing = filtered * math.exp(-9 * t) * (0.55 + 0.45 * math.sin(t * 190))
        hollow = math.sin(math.tau * 43 * t) * math.exp(-7 * t)
        sting.append((impact * 0.20 + tearing * 0.40 + hollow * 0.11) * attack * min((0.65 - t) / 0.06, 1.))
    write('pocket-enter.pcm', sting)

if __name__ == '__main__':
    main()
