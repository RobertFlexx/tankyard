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
failures = os.environ.get("TANKYARD_FAILURES")
if failures:
    counter = Path(failures)
    left = int(counter.read_text())
    if left > 0:
        counter.write_text(str(left - 1))
        print("504", end="")
        sys.exit(22)
fixtures = Path(os.environ["TANKYARD_FIXTURES"])
if url.startswith("https://api.github.com/"):
    if os.environ.get("TANKYARD_REQUIRE_TOKEN"):
        assert "Authorization: Bearer test-token" in args
    names = sorted(path.name for path in fixtures.iterdir())
    output = Path(args[args.index("--output") + 1])
    if "/releases/assets/" in url:
        assert "Accept: application/octet-stream" in args
        source = fixtures / names[int(url.rsplit("/", 1)[-1])]
    else:
        assets = ",".join(
            f'{{"url": "https://api.github.com/repos/RobertFlexx/tankyard/releases/assets/{index}", '
            f'"id": {index}, "name": "{name}", "uploader": {{"url": "https://api.github.com/users/x"}}}}'
            for index, name in enumerate(names))
        output.write_text(f'{{"url": "https://api.github.com/repos/RobertFlexx/tankyard/releases/1", '
                          f'"name": "Tank Yard v1.2.3", "assets": [{assets}]}}')
        sys.exit(0)
elif os.environ.get("TANKYARD_GITHUB_DOWN"):
    assert not any(arg.startswith("Authorization") for arg in args)
    print("504", end="")
    sys.exit(22)
else:
    source = fixtures / url.rsplit("/", 1)[-1]
if not source.is_file():
    sys.exit(22)
shutil.copyfile(source, args[args.index("--output") + 1])
""")
        curl.chmod(0o755)
        uname = self.shims / "uname"
        uname.write_text("""#!/bin/sh
case "$1" in -s) printf '%s\\n' "${TANKYARD_TEST_SYSTEM:-Linux}" ;; -m) printenv TANKYARD_TEST_ARCH ;; -r) echo 15.1-RELEASE ;; esac
""")
        uname.chmod(0o755)
        self.env = dict(os.environ,
            PATH=str(self.shims) + os.pathsep + os.environ["PATH"],
            TANKYARD_FIXTURES=str(self.release), TANKYARD_TEST_ARCH="x86_64",
            XDG_DATA_HOME=str(self.root / "data"))

    def use_freebsd(self):
        """Runs the installer as on a fresh FreeBSD: fetch(1) and sha256(1), no curl."""
        base = self.root / "base"
        freebsd = self.root / "freebsd"
        base.mkdir()
        freebsd.mkdir()
        hidden = {"curl", "sha256sum", "shasum", "uname", "ldd", "fetch", "sha256", "freebsd-version"}
        for directory in ("/usr/bin", "/bin"):
            for entry in Path(directory).iterdir():
                if entry.name not in hidden and not os.path.lexists(base / entry.name):
                    (base / entry.name).symlink_to(entry)
        shutil.copy2(self.shims / "uname", freebsd / "uname")
        scripts = {
            "fetch": """#!/usr/bin/env python3
import os, shutil, sys
from pathlib import Path
args = sys.argv[1:]
url, output = args[-1], Path(args[args.index("-o") + 1])
assert url.startswith("https://")
fixtures = Path(os.environ["TANKYARD_FIXTURES"])
def fail(message):
    print(f"fetch: {url}: {message}", file=sys.stderr)
    sys.exit(1)
accept = os.environ.get("HTTP_ACCEPT", "")
if url.startswith("https://api.github.com/"):
    names = sorted(path.name for path in fixtures.iterdir())
    if "/releases/assets/" in url:
        assert accept == "application/octet-stream", accept
        source = fixtures / names[int(url.rsplit("/", 1)[-1])]
    else:
        assert accept == "application/vnd.github+json", accept
        assets = ",".join(
            f'{{"url": "https://api.github.com/repos/RobertFlexx/tankyard/releases/assets/{index}", '
            f'"id": {index}, "name": "{name}"}}' for index, name in enumerate(names))
        output.write_text(f'{{"name": "Tank Yard v1.2.3", "assets": [{assets}]}}')
        sys.exit(0)
elif os.environ.get("TANKYARD_GITHUB_DOWN"):
    fail("Gateway Time-out")
else:
    source = fixtures / url.rsplit("/", 1)[-1]
if not source.is_file():
    fail("Not Found")
shutil.copyfile(source, output)
""",
            "sha256": """#!/usr/bin/env python3
import hashlib, sys
assert sys.argv[1] == "-q"
print(hashlib.sha256(open(sys.argv[2], "rb").read()).hexdigest())
""",
            "freebsd-version": """#!/bin/sh
printf '%s\\n' "${TANKYARD_TEST_RELEASE:-15.1-RELEASE}"
""",
            "ldd": """#!/bin/sh
printf '%s:\\n' "$1"
printf '%b' "${TANKYARD_TEST_LDD:-\\tlibc.so.7 => /lib/libc.so.7 (0x1)\\n}"
""",
        }
        for name, text in scripts.items():
            (freebsd / name).write_text(text)
            (freebsd / name).chmod(0o755)
        packaging.package(self.binaries, self.release, "freebsd-x86_64", "v1.2.3")
        self.write_checksums()
        self.env.update(PATH=f"{freebsd}{os.pathsep}{base}", TANKYARD_TEST_SYSTEM="FreeBSD", TANKYARD_TEST_ARCH="amd64")

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

    def test_busy_server_responses_are_retried(self):
        counter = self.root / "failures"
        counter.write_text("2")
        self.env["TANKYARD_FAILURES"] = str(counter)
        result = self.run_installer("--no-desktop")
        self.assertIn("retrying", result.stderr)
        self.assertEqual(counter.read_text(), "0")
        self.assertTrue((self.destination / "tankyard").is_file())

    def test_retries_give_up_after_the_configured_attempts(self):
        counter = self.root / "failures"
        counter.write_text("99")
        self.env["TANKYARD_FAILURES"] = str(counter)
        self.env["TANKYARD_DOWNLOAD_ATTEMPTS"] = "2"
        result = self.run_installer("--no-desktop", success=False)
        self.assertIn("Could not reach the latest release", result.stderr)
        # Two tries on github.com, then two through the API.
        self.assertEqual(counter.read_text(), "95")
        self.assertFalse(self.destination.exists())

    def test_downloads_switch_to_the_api_while_github_downloads_fail(self):
        self.env["TANKYARD_GITHUB_DOWN"] = "1"
        result = self.run_installer("--no-desktop")
        self.assertEqual(result.stderr.count("through the GitHub API"), 1, result.stderr)
        result = subprocess.run([str(self.destination / "tankyard"), "--version"], text=True, capture_output=True, check=True)
        self.assertEqual(result.stdout.strip(), "tankyard 1.2.3")
        self.run_installer("--version", "v1.2.3", "--no-desktop")

    def test_api_requests_send_an_optional_token(self):
        self.env["TANKYARD_GITHUB_DOWN"] = "1"
        self.env["TANKYARD_GITHUB_TOKEN"] = "test-token"
        self.env["TANKYARD_REQUIRE_TOKEN"] = "1"
        self.run_installer("--no-desktop")

    def test_damaged_api_download_is_rejected(self):
        self.env["TANKYARD_GITHUB_DOWN"] = "1"
        self.archive.write_bytes(b"damaged download")
        result = self.run_installer("--no-desktop", success=False)
        self.assertIn("Checksum mismatch", result.stderr)

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

    def test_freebsd_installs_with_fetch_and_sha256(self):
        self.use_freebsd()
        result = self.run_installer("--no-desktop")
        self.assertIn("for freebsd-x86_64", result.stdout)
        installed = subprocess.run([str(self.destination / "tankyard"), "--version"], text=True, capture_output=True, check=True)
        self.assertEqual(installed.stdout.strip(), "tankyard 1.2.3")
        result = self.run_installer("--server")
        self.assertIn("hosting.md#on-freebsd", result.stdout)
        self.assertTrue((self.destination / "tankyard-relay").is_file())

    def test_freebsd_switches_to_the_api_with_fetch(self):
        self.use_freebsd()
        self.env["TANKYARD_GITHUB_DOWN"] = "1"
        result = self.run_installer("--no-desktop")
        self.assertIn("through the GitHub API", result.stderr)
        self.assertTrue((self.destination / "tankyard").is_file())

    def test_freebsd_missing_sound_libraries_name_the_package(self):
        self.use_freebsd()
        self.env["TANKYARD_TEST_LDD"] = "\\tlibasound.so.2 => not found (0)\\n"
        result = self.run_installer("--no-desktop", success=False)
        self.assertIn("pkg install alsa-plugins", result.stderr)
        self.assertFalse(self.destination.exists())

    def test_freebsd_14_is_sent_to_the_source_build(self):
        self.use_freebsd()
        self.env["TANKYARD_TEST_RELEASE"] = "14.3-RELEASE-p4"
        result = self.run_installer("--no-desktop", success=False)
        self.assertIn("FreeBSD 15.0 or newer", result.stderr)

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

    def test_packaged_host_launcher_creates_config_then_preserves_arguments(self):
        extracted = self.root / "server folder with spaces"
        with tarfile.open(self.archive) as archive:
            archive.extractall(extracted, filter="data")
        payload = extracted / "tankyard-v1.2.3-linux-x86_64"
        server = payload / "tankyard-server"
        server.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > invocation.txt\n')
        launcher = payload / "host-server.sh"
        env = dict(self.env, TANKYARD_CONFIG="custom arena.toml")
        first = subprocess.run(
            ["sh", str(launcher)], cwd=self.root, env=env,
            text=True, capture_output=True, check=True)
        self.assertIn("Wrote custom arena.toml", first.stdout)
        self.assertEqual(
            (payload / "custom arena.toml").read_bytes(),
            (payload / "configs/server.toml").read_bytes())
        self.assertFalse((payload / "invocation.txt").exists())
        subprocess.run(
            ["sh", str(launcher), "--name", "Friends arena", "--port", "5001"],
            cwd=self.root, env=env, check=True)
        self.assertEqual(
            (payload / "invocation.txt").read_text().splitlines(),
            ["--config", "custom arena.toml", "--name", "Friends arena", "--port", "5001"])

    def test_packages_have_licenses_examples_and_no_private_config(self):
        with tarfile.open(self.archive) as archive:
            names = archive.getnames()
            self.assertTrue(any(name.endswith("/licenses/DejaVu.txt") for name in names))
            self.assertTrue(any(name.endswith("/licenses/Lucide.txt") for name in names))
            self.assertTrue(any(name.endswith("/licenses/Opus.txt") for name in names))
            self.assertTrue(any(name.endswith("/configs/server.toml") for name in names))
            self.assertTrue(any(name.endswith("/configs/tankyard.service") for name in names))
            self.assertTrue(any(name.endswith("/host-server.sh") for name in names))
            self.assertFalse(any("/.git/" in name or "private" in name for name in names))
        archive = packaging.package(self.binaries, self.release, "freebsd-x86_64", "v1.2.3")
        with tarfile.open(archive) as bundle:
            names = bundle.getnames()
            self.assertTrue(any(name.endswith("/configs/tankyard.rc") for name in names))
            self.assertFalse(any(name.endswith(".service") for name in names))
            self.assertTrue(any(name.endswith("/host-server.sh") for name in names))
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
