"""Exercise the real Unix installer with local release fixtures."""
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("package_release", ROOT / "scripts/package_release.py")
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="tankyard-installer-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.release = self.root / "release"
        self.binaries = self.root / "binaries"
        self.shims = self.root / "shims"
        self.destination = self.root / 'custom install "with spaces"'
        for directory in (self.release, self.binaries, self.shims):
            directory.mkdir()
        for binary in packaging.BINARIES:
            path = self.binaries / binary
            path.write_text(f"#!/bin/sh\nprintf '{binary} 1.2.3\\n'\n")
            path.chmod(0o755)
        self.archive = packaging.package(self.binaries, self.release, "linux-x86_64", "v1.2.3")
        self.write_checksums()
        curl = self.shims / "curl"
        curl.write_text("""#!/usr/bin/env python3
import os, shutil, sys
from pathlib import Path
args = sys.argv[1:]
assert "--proto" in args and "=https" in args
url = args[-1]
if url.endswith("/latest"):
    print("https://github.com/RobertFlexx/tankyard/releases/tag/v1.2.3", end="")
    sys.exit(0)
source = Path(os.environ["TANKYARD_FIXTURES"]) / url.rsplit("/", 1)[-1]
if not source.is_file():
    sys.exit(22)
shutil.copyfile(source, args[args.index("--output") + 1])
""")
        curl.chmod(0o755)
        uname = self.shims / "uname"
        uname.write_text("""#!/bin/sh
case "$1" in -s) printf '%s\\n' Linux ;; -m) printenv TANKYARD_TEST_ARCH ;; esac
""")
        uname.chmod(0o755)
        self.env = dict(os.environ,
            PATH=str(self.shims) + os.pathsep + os.environ["PATH"],
            TANKYARD_FIXTURES=str(self.release), TANKYARD_TEST_ARCH="x86_64",
            XDG_DATA_HOME=str(self.root / "data"))

    def write_checksums(self):
        lines = []
        for path in sorted(self.release.glob("*.tar.gz")):
            lines.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
        (self.release / "SHA256SUMS").write_text("".join(lines))

    def run_installer(self, *args, success=True):
        result = subprocess.run(
            ["sh", str(ROOT / "install.sh"), "--bin-dir", str(self.destination), *args],
            env=self.env, text=True, capture_output=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def test_latest_installs_and_reinstalls_without_touching_settings(self):
        self.destination.mkdir()
        settings = self.destination / "settings.json"
        settings.write_text('{"player_name":"RobertFlexx"}')
        self.run_installer("--no-desktop")
        before = (self.destination / "tankyard").read_bytes()
        self.run_installer("--version", "v1.2.3", "--no-desktop")
        self.assertEqual((self.destination / "tankyard").read_bytes(), before)
        self.assertEqual(settings.read_text(), '{"player_name":"RobertFlexx"}')
        self.assertFalse((self.destination / "tankyard-server").exists())
        result = subprocess.run([str(self.destination / "tankyard"), "--version"], text=True, capture_output=True, check=True)
        self.assertEqual(result.stdout.strip(), "tankyard 1.2.3")

    def test_checksum_failure_keeps_an_existing_install(self):
        self.destination.mkdir()
        current = self.destination / "tankyard"
        current.write_text("old working game")
        self.archive.write_bytes(b"damaged download")
        result = self.run_installer("--no-desktop", success=False)
        self.assertIn("Checksum mismatch", result.stderr)
        self.assertEqual(current.read_text(), "old working game")

    def test_missing_release_download_does_not_create_an_install(self):
        self.archive.unlink()
        self.run_installer("--no-desktop", success=False)
        self.assertFalse(self.destination.exists())

    def test_unusable_binary_is_rejected_before_replacing_the_game(self):
        (self.binaries / "tankyard").write_text("#!/bin/sh\nexit 1\n")
        packaging.package(self.binaries, self.release, "linux-x86_64", "v1.2.3")
        self.write_checksums()
        self.destination.mkdir()
        current = self.destination / "tankyard"
        current.write_text("old working game")
        self.run_installer("--no-desktop", success=False)
        self.assertEqual(current.read_text(), "old working game")

    def test_server_install_does_not_replace_the_game(self):
        self.destination.mkdir()
        (self.destination / "tankyard").write_text("existing game")
        self.run_installer("--server")
        self.assertTrue((self.destination / "tankyard-server").is_file())
        self.assertTrue((self.destination / "tankyard-relay").is_file())
        self.assertEqual((self.destination / "tankyard").read_text(), "existing game")

    def test_linux_arm_selects_the_matching_archive(self):
        packaging.package(self.binaries, self.release, "linux-aarch64", "v1.2.3")
        self.write_checksums()
        self.env["TANKYARD_TEST_ARCH"] = "aarch64"
        self.run_installer("--no-desktop")
        self.assertTrue((self.destination / "tankyard").is_file())

    def test_desktop_shortcut_quotes_custom_paths(self):
        self.run_installer()
        shortcut = (self.root / "data/applications/tankyard.desktop").read_text()
        self.assertIn('Exec="', shortcut)
        self.assertIn(r'\"with spaces\"', shortcut)
        self.assertTrue((self.root / "data/icons/hicolor/scalable/apps/tankyard.svg").is_file())
        icon = self.root / "data/icons/hicolor/256x256/apps/tankyard.png"
        self.assertEqual(icon.read_bytes(), (ROOT / "assets/icons/tankyard.png").read_bytes())
        self.assertIn(f"Icon={icon}\n", shortcut)
        self.assertIn("StartupWMClass=tankyard\n", shortcut)

    def test_packages_have_licenses_examples_and_no_private_config(self):
        with tarfile.open(self.archive) as archive:
            names = archive.getnames()
            self.assertTrue(any(name.endswith("/licenses/DejaVu.txt") for name in names))
            self.assertTrue(any(name.endswith("/licenses/Lucide.txt") for name in names))
            self.assertTrue(any(name.endswith("/licenses/Opus.txt") for name in names))
            self.assertTrue(any(name.endswith("/configs/server.toml") for name in names))
            self.assertFalse(any("/.git/" in name or "private" in name for name in names))
        for binary in packaging.BINARIES:
            shutil.copyfile(self.binaries / binary, self.binaries / (binary + ".exe"))
        archive = packaging.package(self.binaries, self.release, "windows-x86_64", "v1.2.3")
        with zipfile.ZipFile(archive) as bundle:
            self.assertTrue(any(name.endswith("/tankyard.exe") for name in bundle.namelist()))
            self.assertTrue(any(name.endswith("/licenses/Lucide.txt") for name in bundle.namelist()))
            self.assertTrue(any(name.endswith("/licenses/Opus.txt") for name in bundle.namelist()))
            self.assertTrue(any(name.endswith("/install.ps1") for name in bundle.namelist()))


if __name__ == "__main__":
    unittest.main()
