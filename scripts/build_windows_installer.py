#!/usr/bin/env python3
"""Compile a current-user graphical Windows setup from the verified release ZIP."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def nsis_path(path: Path) -> str:
    """Quote NSIS metacharacters, independent of shell escaping."""
    return str(path).replace("$", "$$").replace('"', '$\\"')


def build(archive: Path, version: str, output: Path, compiler: str) -> Path:
    if not re.fullmatch(r"v\d+\.\d+\.\d+", version):
        raise ValueError("installer requires a stable v-prefixed version")
    output.mkdir(parents=True, exist_ok=True)
    setup = (output / f"tankyard-{version}-windows-x86_64-setup.exe").resolve()
    with tempfile.TemporaryDirectory(prefix="tankyard-setup-") as directory:
        work = Path(directory)
        expected = f"tankyard-{version}-windows-x86_64"
        with zipfile.ZipFile(archive) as package:
            for item in package.infolist():
                parts = Path(item.filename).parts
                if not parts or parts[0] != expected or ".." in parts or "\\" in item.filename:
                    raise ValueError("unsafe or unexpected package path")
            package.extractall(work)
        payload = work / expected
        for filename in ("tankyard.exe", "tankyard.ico", "LICENSE-MIT", "VERSION"):
            if not (payload / filename).is_file():
                raise ValueError(f"missing installer payload: {filename}")
        if (payload / "VERSION").read_text().strip() != version:
            raise ValueError("installer payload version mismatch")
        lines = []
        for path in sorted(payload.rglob("*")):
            if path.is_file() and path.relative_to(payload).parts[0] != "configs":
                relative = str(path.relative_to(payload)).replace("/", "\\")
                lines.append(f'Delete "$INSTDIR\\{nsis_path(Path(relative))}"')
        for path in sorted((p for p in payload.rglob("*") if p.is_dir() and p.relative_to(payload).parts[0] != "configs"), key=lambda p: len(p.parts), reverse=True):
            relative = str(path.relative_to(payload)).replace("/", "\\")
            lines.append(f'RMDir "$INSTDIR\\{nsis_path(Path(relative))}"')
        manifest = work / "uninstall-files.nsh"
        manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
        prefix = "/" if os.name == "nt" else "-"
        subprocess.run([compiler, f"{prefix}V3", f"{prefix}DVERSION={version[1:]}",
                        f"{prefix}DPAYLOAD_DIR={nsis_path(payload)}",
                        f"{prefix}DOUTPUT_FILE={nsis_path(setup)}",
                        f"{prefix}DUNINSTALL_FILES={nsis_path(manifest)}",
                        str(ROOT / "scripts/windows-installer.nsi")], check=True)
    with setup.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    setup.with_name(setup.name + ".sha256").write_text(f"{digest}  {setup.name}\n", encoding="utf-8")
    return setup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--compiler", default=shutil.which("makensis") or r"C:\Program Files (x86)\NSIS\makensis.exe")
    args = parser.parse_args()
    archive = args.output / f"tankyard-{args.version}-windows-x86_64.zip"
    print(build(archive, args.version, args.output, args.compiler))


if __name__ == "__main__":
    main()
