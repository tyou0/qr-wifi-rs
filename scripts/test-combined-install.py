#!/usr/bin/env python3
"""Run the real installers in temporary destinations, including upgrade/failure paths."""
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
WINDOWS = os.name == "nt"
MAC = platform.system() == "Darwin"


class CombinedInstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="qr-wifi-install-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.payload = self.base / "release with spaces"
        self.bin = self.payload / "bin"
        self.bin.mkdir(parents=True)
        self.destination = self.base / "installed with spaces"
        self.apps = self.base / "applications"
        self.suffix = ".exe" if WINDOWS else ""
        self.names = ("qr-wifi", "qr-wifi-tui", "qr-wifi-host", "qr-wifi-gui")
        for name in self.names:
            binary = self.bin / (name + self.suffix)
            binary.write_text("#!/bin/sh\nexit 0\n")
            binary.chmod(0o755)
        arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x86_64"
        os_name = "windows" if WINDOWS else "macos" if MAC else "linux"
        (self.payload / "platform.txt").write_text(f"{os_name}-{arch}\n")
        shutil.copy(ROOT / "src-tauri/icons/128x128.png", self.payload / "icon.png")
        if MAC:
            contents = self.payload / "QR Wi-Fi RS.app/Contents"
            (contents / "MacOS").mkdir(parents=True)
            (contents / "Info.plist").write_text("<plist></plist>")
            shutil.copy(self.bin / "qr-wifi-gui", contents / "MacOS/qr-wifi-gui")

    def run_installer(self, packaged=False):
        filename = "install.ps1" if WINDOWS else "install.sh"
        script = ROOT / "scripts" / filename
        if packaged:
            shutil.copy(script, self.payload / filename)
            script = self.payload / filename
        if WINDOWS:
            command = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(script),
                       "-InstallDirectory", str(self.destination), "-ShortcutDirectory", str(self.apps), "-NoPathUpdate"]
            if not packaged:
                command += ["-PrebuiltDirectory", str(self.payload)]
        else:
            command = ["bash", str(script), "--prefix", str(self.destination), "--applications-dir", str(self.apps)]
            if not packaged:
                command += ["--prebuilt", str(self.payload)]
        return subprocess.run(command, capture_output=True, text=True, cwd=self.base)

    def test_install_and_upgrade_from_extracted_archive(self):
        for attempt in range(2):
            result = self.run_installer(packaged=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for name in self.names:
                installed = self.destination / "bin" / (name + self.suffix)
                self.assertEqual(installed.read_bytes(), (self.bin / (name + self.suffix)).read_bytes())
            (self.bin / ("qr-wifi" + self.suffix)).write_text(f"#!/bin/sh\n# update {attempt}\n")
        if WINDOWS:
            self.assertTrue((self.apps / "QR Wi-Fi RS.lnk").is_file())
        elif MAC:
            self.assertTrue((self.apps / "QR Wi-Fi RS.app/Contents/MacOS/qr-wifi-gui").is_file())
        else:
            launcher = self.destination / "share/applications/com.thetomyou.qrwifirs.desktop"
            self.assertIn(f'Exec="{self.destination}/bin/qr-wifi-gui"', launcher.read_text())
            if shutil.which("desktop-file-validate"):
                subprocess.run(["desktop-file-validate", str(launcher)], check=True)

    def test_missing_artifact_does_not_modify_installation(self):
        (self.bin / ("qr-wifi-tui" + self.suffix)).unlink()
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_wrong_platform_rejected_before_installation(self):
        (self.payload / "platform.txt").write_text("unsupported-x86_64\n")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.destination.exists())


if __name__ == "__main__":
    unittest.main()
