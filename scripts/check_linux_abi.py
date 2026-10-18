#!/usr/bin/env python3
"""Check imported symbol versions in the compatible x86-64 Linux release."""
import argparse
from pathlib import Path
import re
import subprocess


def check(directory: Path) -> None:
    for name in ("tankyard", "tankyard-server", "tankyard-relay", "tankyard-load", "tankyard-tools"):
        binary = directory / name
        versions = subprocess.check_output(["readelf", "--version-info", str(binary)], text=True)
        glibc = {tuple(map(int, version.split("."))) for version in re.findall(r"Name: GLIBC_([\d.]+)", versions)}
        alsa = {tuple(map(int, re.findall(r"\d+", version))) for version in re.findall(r"Name: ALSA_([\w.]+)", versions)}
        if not glibc or max(glibc) > (2, 31) or "GLIBC_ABI_DT_RELR" in versions:
            raise SystemExit(f"{name}: imported glibc ABI exceeds 2.31")
        if alsa and max(alsa) > (1, 2, 2):
            raise SystemExit(f"{name}: imported ALSA ABI exceeds 1.2.2")
        print(f"{name}: GLIBC_{'.'.join(map(str, max(glibc)))}; ALSA imports {'present' if alsa else 'none'}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary_dir", type=Path)
    check(parser.parse_args().binary_dir)
