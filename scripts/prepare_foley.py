#!/usr/bin/env python3
"""Check shipped foley; --rebuild recreates it from pinned CC0 sources with FFmpeg."""
import argparse
import array
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FOLEY = ROOT / "assets/audio/foley"
FILTER = ("silenceremove=start_periods=1:start_duration=0.004:start_threshold=-45dB,"
          "highpass=f=35,lowpass=f=10000,afade=t=in:d=0.003")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def fetch(url, maximum=32_000_000):
    request = urllib.request.Request(url, headers={"User-Agent": "TankYardAssetTools/1.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        data = response.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError(f"Source exceeds {maximum} bytes: {url}")
    return data


def convert(data):
    decoded = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", "pipe:0", "-t", "4", "-ac", "1", "-ar", "48000",
         "-af", FILTER, "-f", "s16le", "pipe:1"],
        input=data, capture_output=True, check=True,
    ).stdout
    values = array.array("h")
    values.frombytes(decoded)
    if sys.byteorder != "little":
        values.byteswap()
    if len(values) < 100:
        raise ValueError("Source recording is empty")
    peak = max(abs(x) for x in values)
    values = array.array("h", (round(x * 25000 / max(peak, 1)) for x in values))
    fade = min(2400, len(values) // 8)
    for i in range(fade):
        values[-1-i] = round(values[-1-i] * (i / max(1, fade-1)) ** 2)
    if sys.byteorder != "little":
        values.byteswap()
    return values.tobytes()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rebuild", action="store_true")
    args = parser.parse_args()
    manifest = json.loads((FOLEY / "sources.json").read_text())
    archives = {}
    for clip in manifest["clips"]:
        path = FOLEY / clip["file"]
        if args.rebuild:
            source = manifest["sources"][clip["pack"]]
            if "download" in source:
                if clip["pack"] not in archives:
                    data = fetch(source["download"])
                    if digest(data) != source["archive_sha256"]:
                        raise ValueError(f"Archive checksum mismatch: {clip['pack']}")
                    archives[clip["pack"]] = zipfile.ZipFile(io.BytesIO(data))
                data = archives[clip["pack"]].read(clip["source_file"])
            else:
                data = fetch("https://opengameart.org/sites/default/files/" + clip["source_file"])
            if digest(data) != clip["source_sha256"]:
                raise ValueError(f"Recording checksum mismatch: {clip['file']}")
            pcm = convert(data)
            if digest(pcm) != clip["pcm_sha256"]:
                raise ValueError(f"Conversion differs for {clip['file']}; check the FFmpeg version")
            path.write_bytes(pcm)
        if digest(path.read_bytes()) != clip["pcm_sha256"]:
            raise ValueError(f"PCM checksum mismatch: {clip['file']}")
    print(f"Verified {len(manifest['clips'])} foley recordings")


if __name__ == "__main__":
    main()
