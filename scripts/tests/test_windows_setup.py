"""Validate the graphical installer's package boundary and exact uninstall list."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("windows_setup", ROOT / "scripts/build_windows_installer.py")
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class WindowsSetupTests(unittest.TestCase):
    def test_rejects_path_traversal_and_mismatched_payload_versions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "fixture.zip"
            for filename in ("../escape.exe", "tankyard-v1.2.3-windows-x86_64/../escape.exe", "wrong/game.exe"):
                with zipfile.ZipFile(archive, "w") as package:
                    package.writestr(filename, b"MZ")
                with self.assertRaises(ValueError):
                    setup.build(archive, "v1.2.3", root, "never-invoked")
            with zipfile.ZipFile(archive, "w") as package:
                for name in ("tankyard.exe", "tankyard.ico", "LICENSE-MIT", "VERSION"):
                    package.writestr(f"tankyard-v1.2.3-windows-x86_64/{name}", "v9.9.9")
            with self.assertRaisesRegex(ValueError, "version mismatch"):
                setup.build(archive, "v1.2.3", root, "never-invoked")

    def test_uninstall_list_names_only_packaged_files_and_escapes_nsis_variables(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "fixture.zip"
            with zipfile.ZipFile(archive, "w") as package:
                for name in ("tankyard.exe", "tankyard.ico", "LICENSE-MIT", "VERSION", "docs/$user-file.txt"):
                    package.writestr(f"tankyard-v1.2.3-windows-x86_64/{name}", "v1.2.3")
            def compile_fixture(args, check):
                self.assertTrue(check)
                manifest = Path(next(a.split("=", 1)[1] for a in args if "DUNINSTALL_FILES=" in a))
                text = manifest.read_text()
                self.assertIn('Delete "$INSTDIR\\docs\\$$user-file.txt"', text)
                self.assertNotIn("RMDir /r", text)
                output = Path(next(a.split("=", 1)[1] for a in args if "DOUTPUT_FILE=" in a))
                output.write_bytes(b"MZ setup fixture")
            with patch.object(setup.subprocess, "run", side_effect=compile_fixture):
                result = setup.build(archive, "v1.2.3", root, "fixture-compiler")
            self.assertTrue(result.is_file())
            self.assertIn(result.name, result.with_name(result.name + ".sha256").read_text())
