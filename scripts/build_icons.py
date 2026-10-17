#!/usr/bin/env python3
"""Rebuild native application icons from the repository SVG (ImageMagick needed)."""
from pathlib import Path
import shutil
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    output = ROOT / "assets/icons"
    output.mkdir(exist_ok=True)
    convert = shutil.which("magick") or shutil.which("convert")
    if convert is None:
        raise SystemExit("Install ImageMagick to rebuild icons.")
    images = {}
    for size in (16, 32, 48, 64, 128, 256, 512, 1024):
        png = subprocess.check_output([
            convert, "-background", "none", "-density", "768",
            str(ROOT / "assets/tankyard.svg"), "-resize", f"{size}x{size}",
            "-depth", "8", "PNG32:-",
        ])
        images[size] = png
        if size == 256:
            (output / "tankyard.png").write_bytes(png)
        if size in (16, 32, 64):
            rgba = subprocess.check_output([convert, "-", "-depth", "8", "RGBA:-"], input=png)
            assert len(rgba) == size * size * 4
            (output / f"tankyard-{size}.rgba").write_bytes(rgba)
    sizes = (16, 32, 48, 64, 128, 256)
    offset = 6 + 16 * len(sizes)
    ico = bytearray(struct.pack("<HHH", 0, 1, len(sizes)))
    for size in sizes:
        ico += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(images[size]), offset)
        offset += len(images[size])
    (output / "tankyard.ico").write_bytes(ico + b"".join(images[size] for size in sizes))
    chunks = b"".join(tag + struct.pack(">I", len(images[size]) + 8) + images[size]
        for tag, size in ((b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)))
    (output / "tankyard.icns").write_bytes(b"icns" + struct.pack(">I", len(chunks) + 8) + chunks)


if __name__ == "__main__":
    main()
