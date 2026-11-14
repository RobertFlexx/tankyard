"""Checks freshly built release binaries before they are packaged.

python scripts/check_release_binaries.py --binary-dir DIR --version vX.Y.Z --platform PLATFORM

Every binary must start and report the version, the bundled mods must load,
and a Lua script error must unwind through the linked Luau (C++) into a
reported failure. The release workflow runs this on each platform.
"""
import argparse
import os
import subprocess
import tempfile
from pathlib import Path

BINARIES = ("tankyard", "tankyard-server", "tankyard-relay", "tankyard-load", "tankyard-tools")
PACKS = ("fast_arena", "vampire", "bounty_hunt", "gravity_wells", "sprinkler", "workshop_toolkit", "logic_kit", "arena_fx", "effects_lab")
ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary-dir", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", required=True)
    args = parser.parse_args()
    suffix = ".exe" if os.name == "nt" else ""
    for name in BINARIES:
        executable = args.binary_dir / (name + suffix)
        result = subprocess.run([str(executable), "--version"], check=True, text=True, capture_output=True)
        assert args.version.removeprefix("v") in result.stdout, result.stdout
        print(result.stdout.strip())
    if args.platform in ("linux-x86_64", "windows-x86_64"):
        game = (args.binary_dir / ("tankyard" + suffix)).read_bytes()
        assert b"libnvidia-ngx-dlss.so" in game or b"nvngx_dlss.dll" in game, "DLSS is missing"
        print("DLSS is linked")
    tools = args.binary_dir / ("tankyard-tools" + suffix)
    subprocess.run([str(tools), "validate-mods", *(arg for pack in PACKS for arg in ("--mod", str(ROOT / "mods" / pack)))],
                   check=True)
    with tempfile.TemporaryDirectory(prefix="tankyard-unwind-") as directory:
        broken = Path(directory)
        (broken / "mod.json").write_text(
            '{"api_version": 2, "id": "unwind_check", "name": "Unwind check", "version": "1", "scripts": ["main.lua"]}',
            encoding="utf-8")
        (broken / "main.lua").write_text(
            'local ok, err = pcall(error, "caught")\nassert(not ok and tostring(err):find("caught"))\nerror("expected failure")\n',
            encoding="utf-8")
        result = subprocess.run([str(tools), "validate-mods", "--mod", str(broken)], text=True, capture_output=True)
    assert result.returncode == 1 and "expected failure" in result.stdout + result.stderr, (
        result.returncode, result.stdout, result.stderr)
    print("Lua scripts load, and script errors are reported")


if __name__ == "__main__":
    main()
