#!/usr/bin/env python3
"""Build the graphical macOS installer (.pkg) from the verified release archive.

The package installs `Tank Yard.app` into /Applications through the standard
Installer app: welcome, license, destination and summary pages. It is built
with Apple's own pkgbuild and productbuild, so it needs a Mac.
"""
import argparse
import hashlib
import html
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent
RESOURCES = ROOT / "scripts/macos-installer"
IDENTIFIER = "me.kokonico.tankyard"


def distribution(version: str, component: str) -> str:
    number = version.removeprefix("v")
    return f"""<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>Tank Yard {html.escape(number)}</title>
    <welcome file="welcome.html" mime-type="text/html"/>
    <license file="LICENSE.txt"/>
    <conclusion file="conclusion.html" mime-type="text/html"/>
    <background file="background.png" alignment="bottomleft" scaling="proportional"/>
    <background-darkAqua file="background.png" alignment="bottomleft" scaling="proportional"/>
    <options customize="never" require-scripts="false" hostArchitectures="arm64,x86_64"/>
    <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>
    <volume-check><allowed-os-versions><os-version min="11.0"/></allowed-os-versions></volume-check>
    <choices-outline><line choice="default"><line choice="{IDENTIFIER}"/></line></choices-outline>
    <choice id="default"/>
    <choice id="{IDENTIFIER}" visible="false"><pkg-ref id="{IDENTIFIER}"/></choice>
    <pkg-ref id="{IDENTIFIER}" version="{html.escape(number)}" onConclusion="none">{html.escape(component)}</pkg-ref>
</installer-gui-script>
"""


def build(archive: Path, version: str, output: Path) -> Path:
    if not re.fullmatch(r"v\d+\.\d+\.\d+", version):
        raise ValueError("installer requires a stable v-prefixed version")
    output.mkdir(parents=True, exist_ok=True)
    installer = (output / f"tankyard-{version}-macos-universal.pkg").resolve()
    expected = f"tankyard-{version}-macos-universal"
    with tempfile.TemporaryDirectory(prefix="tankyard-pkg-") as directory:
        work = Path(directory)
        with tarfile.open(archive, "r:gz") as package:
            for member in package.getmembers():
                parts = Path(member.name).parts
                if not parts or parts[0] != expected or ".." in parts or member.name.startswith("/"):
                    raise ValueError("unsafe or unexpected package path")
                if member.issym() or member.islnk() or member.isdev():
                    raise ValueError("links and devices are not allowed in the package")
            package.extractall(work, filter="data")
        payload = work / expected
        if (payload / "VERSION").read_text().strip() != version:
            raise ValueError("installer payload version mismatch")
        app = payload / "Tank Yard.app"
        for required in ("Contents/MacOS/tankyard", "Contents/MacOS/tankyard-server", "Contents/Info.plist"):
            if not (app / required).is_file():
                raise ValueError(f"missing installer payload: {required}")
        # Only the app is installed; everything it needs is inside it.
        root = work / "root"
        root.mkdir()
        shutil.copytree(app, root / "Tank Yard.app", symlinks=True)
        # Build-machine attributes (quarantine, provenance) would otherwise ship
        # as AppleDouble "._" files.
        subprocess.run(["xattr", "-cr", str(root)], check=True)
        subprocess.run(["codesign", "--verify", "--strict", str(root / "Tank Yard.app")], check=True)
        components = work / "components.plist"
        subprocess.run(["pkgbuild", "--analyze", "--root", str(root), str(components)], check=True)
        with components.open("rb") as stream:
            bundles = plistlib.load(stream)
        for bundle in bundles:
            # Always install into /Applications, even if a copy lives elsewhere.
            bundle["BundleIsRelocatable"] = False
            bundle["BundleOverwriteAction"] = "upgrade"
        with components.open("wb") as stream:
            plistlib.dump(bundles, stream)
        component = "tankyard-app.pkg"
        packages = work / "packages"
        packages.mkdir()
        subprocess.run([
            "pkgbuild", "--root", str(root), "--component-plist", str(components),
            "--identifier", IDENTIFIER, "--version", version.removeprefix("v"),
            "--install-location", "/Applications", str(packages / component),
        ], check=True, env={**os.environ, "COPYFILE_DISABLE": "1"})
        resources = work / "resources"
        resources.mkdir()
        for page in ("welcome.html", "conclusion.html"):
            text = (RESOURCES / page).read_text(encoding="utf-8")
            (resources / page).write_text(text.replace("__VERSION__", html.escape(version)), encoding="utf-8")
        (resources / "LICENSE.txt").write_text(
            (ROOT / "LICENSE-MIT").read_text(encoding="utf-8")
            + "\n\nTank Yard is also available under the Apache License 2.0; see LICENSE-APACHE inside the app.\n",
            encoding="utf-8",
        )
        shutil.copyfile(ROOT / "assets/icons/tankyard.png", resources / "background.png")
        definition = work / "distribution.xml"
        definition.write_text(distribution(version, component), encoding="utf-8")
        subprocess.run([
            "productbuild", "--distribution", str(definition), "--resources", str(resources),
            "--package-path", str(packages), str(installer),
        ], check=True)
    with installer.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    installer.with_name(installer.name + ".sha256").write_text(f"{digest}  {installer.name}\n", encoding="utf-8")
    return installer


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    archive = args.archive or args.output / f"tankyard-{args.version}-macos-universal.tar.gz"
    print(build(archive, args.version, args.output))


if __name__ == "__main__":
    main()
