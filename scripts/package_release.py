#!/usr/bin/env python3
"""Build release archives from already compiled, platform-specific binaries."""
import argparse
import hashlib
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BINARIES = ("tankyard", "tankyard-server", "tankyard-relay", "tankyard-load", "tankyard-tools")
PLATFORMS = ("linux-x86_64", "linux-aarch64", "macos-universal", "windows-x86_64")


def package(binary_dir: Path, output: Path, platform: str, version: str) -> Path:
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?", version):
        raise ValueError("version must be a v-prefixed release tag")
    if platform not in PLATFORMS:
        raise ValueError(f"unsupported platform: {platform}")
    output.mkdir(parents=True, exist_ok=True)
    name = f"tankyard-{version}-{platform}"
    suffix = ".exe" if platform.startswith("windows") else ""
    with tempfile.TemporaryDirectory(prefix="tankyard-package-") as directory:
        payload = Path(directory) / name
        payload.mkdir()
        for binary in BINARIES:
            source = binary_dir / (binary + suffix)
            if not source.is_file():
                raise FileNotFoundError(source)
            target = payload / source.name
            shutil.copyfile(source, target)
            target.chmod(0o755)
        for filename in ("README.md", "NOTICE.md", "LICENSE-MIT", "LICENSE-APACHE", "install.sh", "install.ps1"):
            shutil.copy2(ROOT / filename, payload)
        for directory_name in ("configs", "mods", "licenses"):
            (payload / directory_name).mkdir()
        for filename in ("server.toml", "official.toml"):
            shutil.copy2(ROOT / "configs" / filename, payload / "configs" / filename)
        for mod in ("fast_arena", "vampire"):
            shutil.copytree(ROOT / "mods" / mod, payload / "mods" / mod)
        shutil.copytree(ROOT / "docs", payload / "docs")
        shutil.copy2(ROOT / "assets/fonts/LICENSE-DejaVu.txt", payload / "licenses/DejaVu.txt")
        shutil.copy2(ROOT / "assets/tankyard.svg", payload)
        (payload / "VERSION").write_text(version + "\n", encoding="utf-8")
        if platform == "macos-universal":
            app = payload / "Tank Yard.app/Contents"
            (app / "MacOS").mkdir(parents=True)
            (app / "Resources").mkdir()
            shutil.copy2(payload / "tankyard", app / "MacOS/tankyard")
            with (app / "Info.plist").open("wb") as stream:
                plistlib.dump({
                    "CFBundleName": "Tank Yard",
                    "CFBundleDisplayName": "Tank Yard",
                    "CFBundleIdentifier": "me.kokonico.tankyard",
                    "CFBundleExecutable": "tankyard",
                    "CFBundlePackageType": "APPL",
                    "CFBundleShortVersionString": version.removeprefix("v").split("-")[0],
                    "CFBundleVersion": version.removeprefix("v").split("-")[0],
                    "LSMinimumSystemVersion": "11.0",
                    "NSHighResolutionCapable": True,
                }, stream)
            subprocess.run(["codesign", "--force", "--sign", "-", str(app.parent)], check=True)
            subprocess.run(["codesign", "--verify", "--strict", str(app.parent)], check=True)
        if suffix:
            archive = output / (name + ".zip")
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
                for path in sorted(payload.rglob("*")):
                    if path.is_file():
                        bundle.write(path, path.relative_to(payload.parent))
        else:
            archive = output / (name + ".tar.gz")
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(payload, arcname=name)
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    (output / (archive.name + ".sha256")).write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    with (ROOT / "Cargo.toml").open("rb") as stream:
        version = tomllib.load(stream)["workspace"]["package"]["version"]
    if args.version != f"v{version}":
        parser.error(f"tag {args.version} does not match Cargo version {version}")
    print(package(args.binary_dir, args.output, args.platform, args.version))


if __name__ == "__main__":
    main()
