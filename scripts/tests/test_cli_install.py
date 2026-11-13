import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
INSTALL = ROOT / "scripts/install-cli.py"


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="spielorders-install-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()
        self.prefix = self.base / "prefix with spaces"
        self.profiles = self.base / "zsh"
        self.profiles.mkdir()
        self.environment = dict(os.environ, ZDOTDIR=str(self.profiles))

    def install(self):
        return subprocess.run([sys.executable, str(INSTALL), "--prefix", str(self.prefix)],
                              cwd=ROOT, env=self.environment, capture_output=True, text=True)

    def test_repeat(self):
        self.prefix.mkdir()
        owner_file = self.prefix / "notes.txt"
        owner_file.write_text("Keep these notes\n")
        profile = self.base / "managed-zshrc"
        original = b'export PATH="$HOME/.local/bin:$PATH"\n# User settings without final newline'
        profile.write_bytes(original)
        profile.chmod(0o640)
        (self.profiles / ".zshrc").symlink_to(profile)
        installed_profile = None
        for _ in range(2):
            result = self.install()
            self.assertEqual(result.returncode, 0, result.stderr)
            found = subprocess.run(["/bin/zsh", "-lic", "command -v spielorders"],
                                   env=self.environment, capture_output=True, text=True, check=True)
            self.assertEqual(found.stdout.strip(), str(self.prefix / "bin/spielorders"))
            shell_path = subprocess.run(["/bin/zsh", "-lic", 'print -r -- "$PATH"'],
                                        env=self.environment, capture_output=True, text=True, check=True)
            components = shell_path.stdout.strip().split(":")
            self.assertEqual(components.count(str(self.prefix / "bin")), 1)
            self.assertEqual(components[0], str(self.prefix / "bin"))
            self.assertIn(str(Path.home() / ".local/bin"), components)
            defaults = subprocess.run([str(self.prefix / "bin/spielorders"), "agent", "defaults", "--json"],
                                      capture_output=True, text=True, check=True)
            self.assertIn(str(self.prefix / "share/spielorders/agent-guidance"), defaults.stdout)
            templates = ROOT / "crates/spielorders/agent-guidance"
            for template in templates.rglob("*"):
                if template.is_file():
                    installed = self.prefix / "share/spielorders/agent-guidance" / template.relative_to(templates)
                    self.assertEqual(installed.read_bytes(), template.read_bytes())
            self.assertEqual(owner_file.read_text(), "Keep these notes\n")
            self.assertTrue((self.profiles / ".zshrc").is_symlink())
            self.assertEqual(profile.stat().st_mode & 0o777, 0o640)
            self.assertTrue(profile.read_bytes().startswith(original))
            if installed_profile is not None:
                self.assertEqual(profile.read_bytes(), installed_profile)
            installed_profile = profile.read_bytes()
        self.assertEqual(profile.read_bytes().count(b"# >>> spielorders initialize >>>"), 1)
        self.assertEqual(profile.read_bytes().count(b"# <<< spielorders initialize <<<"), 1)

    def test_block_update(self):
        profile = self.profiles / ".zshrc"
        before = b"# Owner settings\nsetopt auto_cd\n"
        after = b"# Keep these too\nsetopt no_beep\n"
        profile.write_bytes(before + b'# >>> spielorders initialize >>>\nexport PATH="/old-cli:$PATH"\n# <<< spielorders initialize <<<\n' + after)
        result = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        updated = profile.read_bytes()
        self.assertTrue(updated.startswith(before))
        self.assertTrue(updated.endswith(after))
        self.assertNotIn(b"/old-cli", updated)
        self.assertEqual(updated.count(b"# >>> spielorders initialize >>>"), 1)

    def test_conflicts(self):
        self.prefix.mkdir()
        (self.prefix / "bin").mkdir()
        binary = self.prefix / "bin/spielorders"
        binary.write_bytes(b"Owner file, not an installed tool")
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(binary.read_bytes(), b"Owner file, not an installed tool")
        self.assertFalse((self.profiles / ".zshrc").exists())
        binary.unlink()
        profile = self.profiles / ".zshrc"
        original = b"# >>> spielorders initialize >>>\nUnfinished managed block\n"
        profile.write_bytes(original)
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(profile.read_bytes(), original)
        self.assertFalse(binary.exists())

    def test_changed_defaults(self):
        self.assertEqual(self.install().returncode, 0)
        skill = self.prefix / "share/spielorders/agent-guidance/SKILL.md"
        skill.write_text("Locally changed defaults\n")
        profile = (self.profiles / ".zshrc").read_bytes()
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(skill.read_text(), "Locally changed defaults\n")
        self.assertEqual((self.profiles / ".zshrc").read_bytes(), profile)

    @unittest.skipIf(os.geteuid() == 0, "root bypasses the ordinary profile permission refusal")
    def test_readonly_profile(self):
        profile = self.profiles / ".zshrc"
        original = b"# Read-only owner profile\n"
        profile.write_bytes(original)
        profile.chmod(0o400)
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(profile.read_bytes(), original)
        self.assertEqual(profile.stat().st_mode & 0o777, 0o400)
        self.assertFalse((self.prefix / "bin/spielorders").exists())
        self.assertFalse((self.prefix / "env").exists())
        self.assertFalse((self.profiles / ".zprofile").exists())

    def test_path_rollback(self):
        profile = self.profiles / ".zshrc"
        original = b"# Owner settings\nalias spielorders=false\n"
        profile.write_bytes(original)
        profile.chmod(0o640)
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fresh zsh", result.stderr)
        self.assertEqual(profile.read_bytes(), original, result.stderr)
        self.assertEqual(profile.stat().st_mode & 0o777, 0o640)
        self.assertFalse((self.profiles / ".zprofile").exists())
        self.assertFalse((self.prefix / "bin/spielorders").exists())
        self.assertFalse((self.prefix / "env").exists())
        self.assertFalse((self.prefix / ".install.json").exists())

    def test_interrupted_rollback(self):
        profile = self.profiles / ".zshrc"
        original = b"# Owner settings\nalias spielorders=false\n"
        profile.write_bytes(original)
        # Interrupt as rollback removes the new profile, before it restores the original.
        sent = self.base / "signal-sent"
        interrupt = (
            "import os, runpy, signal, sys\n"
            "from pathlib import Path\n"
            "profile, unlink, sent = sys.argv[1], Path.unlink, sys.argv[4]\n"
            "def interrupting(path, *rest, **options):\n"
            "    unlink(path, *rest, **options)\n"
            "    if os.fspath(path) == profile:\n"
            "        Path(sent).write_text('sent')\n"
            "        os.kill(os.getpid(), signal.SIGINT)\n"
            "Path.unlink = interrupting\n"
            "sys.argv = [sys.argv[2], '--prefix', sys.argv[3]]\n"
            "runpy.run_path(sys.argv[0], run_name='__main__')\n")
        result = subprocess.run([sys.executable, "-c", interrupt, str(profile), str(INSTALL), str(self.prefix), str(sent)],
                                cwd=ROOT, env=self.environment, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fresh zsh", result.stderr)
        self.assertTrue(sent.is_file(), "The failure case must really send its interruption")
        self.assertEqual(profile.read_bytes(), original, result.stderr)
        self.assertFalse((self.profiles / ".zprofile").exists())
        for name in ("bin/spielorders", "env", ".install.json", ".install.lock"):
            self.assertFalse((self.prefix / name).exists(), name)
        self.assertEqual([path for path in self.prefix.rglob("*") if path.is_file()], [])

    def test_inherited_path(self):
        for name in (".zprofile", ".zshrc"):
            (self.profiles / name).write_bytes(b"return\n")
        self.environment["PATH"] = f"{self.prefix / 'bin'}:{os.environ['PATH']}"
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fresh zsh", result.stderr)
        self.assertEqual((self.profiles / ".zshrc").read_bytes(), b"return\n")
        self.assertFalse((self.prefix / "bin/spielorders").exists())

    def test_stale_lock(self):
        lock = self.prefix / ".install.lock"
        lock.mkdir(parents=True)
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(str(lock), result.stderr)
        self.assertIn("remove that directory", result.stderr)
        self.assertTrue(lock.is_dir())
        self.assertFalse((self.prefix / "bin/spielorders").exists())
        self.assertFalse((self.profiles / ".zshrc").exists())

    def test_retired_file(self):
        self.assertEqual(self.install().returncode, 0)
        retired = "share/spielorders/agent-guidance/references/retired.md"
        (self.prefix / retired).write_text("A reference an earlier installation owned\n")
        receipt = self.prefix / ".install.json"
        owned = json.loads(receipt.read_text())
        owned[retired] = hashlib.sha256((self.prefix / retired).read_bytes()).hexdigest()
        receipt.write_text(json.dumps(owned))
        result = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.prefix / retired).exists())
        self.assertNotIn(retired, json.loads(receipt.read_text()))
        self.assertTrue((self.prefix / "bin/spielorders").is_file())

    def test_shell_directory(self):
        redirected = self.base / "effective-zsh"
        redirected.mkdir()
        (self.profiles / ".zshenv").write_text(f'ZDOTDIR="{redirected}"\n')
        shared = self.base / "shared-profile"
        original = b"# Shared owner settings\n"
        shared.write_bytes(original)
        for name in (".zprofile", ".zshrc"):
            (redirected / name).symlink_to(shared)
        result = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(shared.read_bytes().startswith(original))
        self.assertEqual(shared.read_bytes().count(b"# >>> spielorders initialize >>>"), 1)
        self.assertFalse((self.profiles / ".zprofile").exists())
        self.assertFalse((self.profiles / ".zshrc").exists())
        for name in (".zprofile", ".zshrc"):
            self.assertTrue((redirected / name).is_symlink())
        found = subprocess.run(["/bin/zsh", "-lic", "command -v spielorders"],
                               env=self.environment, capture_output=True, text=True, check=True)
        self.assertEqual(found.stdout.strip(), str(self.prefix / "bin/spielorders"))

    def test_path_separator(self):
        self.prefix = self.base / "invalid:prefix"
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("PATH separator", result.stderr)
        self.assertFalse(self.prefix.exists())
        self.assertFalse((self.profiles / ".zshrc").exists())

    def test_interrupted(self):
        profile = self.profiles / ".zshrc"
        ready = self.base / "shell-ready"
        pipe = self.base / "shell-input"
        os.mkfifo(pipe)
        original = f': > "{ready}"\nread -r < "{pipe}"\n'.encode()
        profile.write_bytes(original)
        process = subprocess.Popen([sys.executable, str(INSTALL), "--prefix", str(self.prefix)],
                                   cwd=ROOT, env=self.environment, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 10
            while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(ready.exists(), "The fresh shell must reach the owner's profile")
            self.assertTrue((self.prefix / "bin/spielorders").is_file())
            process.terminate()
            _, errors = process.communicate(timeout=10)
            self.assertNotEqual(process.returncode, 0, errors)
            self.assertEqual(profile.read_bytes(), original)
            self.assertFalse((self.profiles / ".zprofile").exists())
            self.assertFalse((self.prefix / "bin/spielorders").exists())
            self.assertFalse((self.prefix / ".install.json").exists())
        finally:
            # Release a shell whose parent was interrupted without settling it.
            try:
                descriptor = os.open(pipe, os.O_WRONLY | os.O_NONBLOCK)
                try:
                    os.write(descriptor, b"continue\n")
                finally:
                    os.close(descriptor)
            except OSError:
                pass
            if process.poll() is None:
                process.kill()
                process.communicate()


if __name__ == "__main__":
    unittest.main()
