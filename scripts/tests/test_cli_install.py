import hashlib
import io
import json
import os
import signal
import tarfile
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
INSTALL = ROOT / "scripts/install-cli.py"
RELEASE_INSTALLER = ROOT / "scripts/install.sh"


def release_archive(directory):
    """The archive and checksum the release workflow makes, built from this checkout."""
    subprocess.run(["cargo", "build", "-q", "-p", "spielorders-cli", "--release", "--locked"],
                   cwd=ROOT, check=True)
    metadata = subprocess.run(["cargo", "metadata", "-q", "--format-version", "1", "--no-deps", "--locked"],
                              cwd=ROOT, check=True, capture_output=True)
    built = Path(json.loads(metadata.stdout)["target_directory"]) / "release/spielorders"
    subprocess.run(["sh", str(ROOT / "scripts/package-cli.sh"), str(built), "macos-arm64", str(directory)],
                   check=True)
    return Path(directory) / "spielorders-macos-arm64.tar.gz"


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="spielorders-install-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()
        self.prefix = self.base / "prefix with spaces"
        self.profiles = self.base / "zsh"
        self.profiles.mkdir()
        self.home = self.base / "home"
        self.home.mkdir()
        # Never the owner's own home or login shell: the installer registers PATH in the
        # profiles of whatever shell SHELL names, under HOME. The Rust toolchain stays
        # where it is installed.
        real_home = Path.home()
        self.environment = dict(
            os.environ, HOME=str(self.home), SHELL="/bin/zsh", ZDOTDIR=str(self.profiles),
            CARGO_HOME=os.environ.get("CARGO_HOME", str(real_home / ".cargo")),
            RUSTUP_HOME=os.environ.get("RUSTUP_HOME", str(real_home / ".rustup")))

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
            self.assertIn(str(self.home / ".local/bin"), components)
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
        receipt.write_text(json.dumps(owned, sort_keys=True, indent=2) + "\n")
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


class ReleaseInstallerTests(unittest.TestCase):
    """The public installer, given a release's own archive rather than a checkout."""

    @classmethod
    def setUpClass(cls):
        cls.releases = tempfile.TemporaryDirectory(prefix="spielorders-release-")
        cls.archive = release_archive(cls.releases.name)

    @classmethod
    def tearDownClass(cls):
        cls.releases.cleanup()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="spielorders-install-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()
        self.prefix = self.base / "prefix"
        self.home = self.base / "home"
        self.home.mkdir()
        self.profiles = self.base / "zsh"
        self.profiles.mkdir()
        self.environment = dict(os.environ, HOME=str(self.home), SHELL="/bin/zsh", ZDOTDIR=str(self.profiles))

    def install(self, *arguments):
        return subprocess.run(["sh", str(RELEASE_INSTALLER), "--prefix", str(self.prefix), *arguments],
                              env=self.environment, capture_output=True, text=True)

    def untouched(self):
        self.assertFalse((self.prefix / "bin/spielorders").exists())
        self.assertFalse((self.prefix / ".install.json").exists())
        self.assertEqual(list(self.profiles.iterdir()), [])
        self.assertEqual(list(self.home.iterdir()), [])

    def test_checksum_mismatch(self):
        tampered = self.base / "spielorders-macos-arm64.tar.gz"
        tampered.write_bytes(self.archive.read_bytes())
        checksum = Path(f"{self.archive}.sha256").read_text()
        Path(f"{tampered}.sha256").write_text(("0" if checksum[0] != "0" else "1") + checksum[1:])
        result = self.install("--archive", str(tampered))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match its checksum", result.stderr)
        self.untouched()

    def test_unsupported_platform(self):
        tools = self.base / "tools"
        tools.mkdir()
        uname = tools / "uname"
        uname.write_text('#!/bin/sh\ncase "$1" in -s) echo SunOS ;; -m) echo sparc64 ;; esac\n')
        uname.chmod(0o755)
        self.environment["PATH"] = f"{tools}:{os.environ['PATH']}"
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported platform: SunOS sparc64", result.stderr)
        self.assertFalse(self.prefix.exists())
        self.untouched()

    def test_bash(self):
        self.environment["SHELL"] = "/bin/bash"
        profile = self.home / ".profile"
        original = b"# Owner settings bash reads at login\n"
        profile.write_bytes(original)
        result = self.install("--archive", str(self.archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.home / ".bash_profile").exists(), "the owner's .profile stays the login file")
        self.assertTrue(profile.read_bytes().startswith(original))
        for shell in (["/bin/bash", "-lic"], ["/bin/bash", "-ic"]):
            found = subprocess.run([*shell, "command -v spielorders"], env=self.environment,
                                   capture_output=True, text=True, check=True)
            self.assertEqual(found.stdout.strip().splitlines()[-1], str(self.prefix / "bin/spielorders"))
        version = subprocess.run([str(self.prefix / "bin/spielorders"), "--version"],
                                 capture_output=True, text=True, check=True)
        self.assertTrue(version.stdout.startswith("spielorders "))

    def test_logout_output(self):
        (self.profiles / ".zlogout").write_text("echo Goodbye\n")
        result = self.install("--archive", str(self.archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.prefix / "bin/spielorders").is_file())

    def test_failed_path_check_leaves_nothing(self):
        profile = self.profiles / ".zshrc"
        original = b"alias spielorders=false\n"
        profile.write_bytes(original)
        result = self.install("--archive", str(self.archive))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fresh zsh", result.stderr)
        self.assertFalse(self.prefix.exists(), "a prefix the installer created goes with the rollback")
        self.assertEqual(profile.read_bytes(), original)
        self.assertEqual(sorted(path.name for path in self.profiles.iterdir()), [".zshrc"])

    def test_failed_update_preserves_unrelated_sidecars(self):
        result = self.install("--archive", str(self.archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        binary = self.prefix / "bin/spielorders"
        original_binary = binary.read_bytes()
        profile = self.profiles / ".zshrc"
        original_profile = b"alias spielorders=false\n"
        profile.write_bytes(original_profile)
        sidecars = [Path(f"{binary}.spielorders-restore"),
                    Path(f"{profile}.spielorders-restore")]
        for sidecar in sidecars:
            sidecar.write_bytes(b"Unrelated owner material\n")

        result = self.install("--archive", str(self.archive))

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fresh zsh", result.stderr)
        self.assertEqual(binary.read_bytes(), original_binary)
        self.assertEqual(profile.read_bytes(), original_profile)
        for sidecar in sidecars:
            self.assertTrue(sidecar.is_file(), f"Rollback removed {sidecar}")
            self.assertEqual(sidecar.read_bytes(), b"Unrelated owner material\n")

    def test_recovery_preserves_unrelated_staging_names(self):
        self.prefix.mkdir()
        unrelated = self.prefix / ".install-ABCDEF"
        unrelated.mkdir()
        note = unrelated / "notes.txt"
        note.write_bytes(b"Unrelated owner material\n")

        result = self.install("--archive", str(self.archive))

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(note.is_file(), "Recovery removed an unrelated directory")
        self.assertEqual(note.read_bytes(), b"Unrelated owner material\n")

    def test_success_settles_its_watchdog(self):
        (self.profiles / ".zshrc").write_text("sleep 1\n")
        tools = self.base / "tools"
        tools.mkdir()
        pids = self.base / "sleep-pids"
        sleeper = tools / "sleep"
        sleeper.write_text(f'#!/bin/sh\nprintf "%s\\n" "$$" >> "{pids}"\nexec /bin/sleep "$@"\n')
        sleeper.chmod(0o755)
        self.environment["PATH"] = f"{tools}:{os.environ['PATH']}"

        result = self.install("--archive", str(self.archive))

        self.assertEqual(result.returncode, 0, result.stderr)
        remaining = []
        started = pids.read_text().splitlines() if pids.exists() else []
        for pid in map(int, started):
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                continue
            remaining.append(pid)
            os.kill(pid, signal.SIGKILL)
        self.assertEqual(remaining, [], "Installation returned with its watchdog still running")

    def test_trailing_slash(self):
        result = subprocess.run(["sh", str(RELEASE_INSTALLER), "--prefix", f"{self.prefix}//",
                                 "--archive", str(self.archive)],
                                env=self.environment, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(b"//", (self.profiles / ".zshrc").read_bytes())

    def test_truncated_download(self):
        script = RELEASE_INSTALLER.read_text()
        call = script.rindex('main "$@"')
        for cut in (call + len("main"), call + len('main "$@"'), len(script) // 2):
            result = subprocess.run(["sh", "-s", "--", "--prefix", str(self.prefix),
                                     "--archive", str(self.archive)],
                                    input=script[:cut], env=self.environment,
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0, f"cut at {cut}")
            self.untouched()
            self.assertFalse(self.prefix.exists())

    def crafted(self, entries, checksum=None):
        """A release archive holding `entries` (name, bytes or a link target), with its checksum."""
        archive = self.base / "spielorders-macos-arm64.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            for name, content in entries:
                info = tarfile.TarInfo(f"spielorders-macos-arm64/{name}")
                if isinstance(content, str):
                    info.type = tarfile.SYMTYPE
                    info.linkname = content
                    bundle.addfile(info)
                else:
                    info.size = len(content)
                    info.mode = 0o755
                    bundle.addfile(info, io.BytesIO(content))
        digest = checksum or hashlib.sha256(archive.read_bytes()).hexdigest()
        Path(f"{archive}.sha256").write_text(f"{digest}  {archive.name}\n")
        return archive

    def test_checksum_is_checked_before_anything_runs(self):
        marker = self.base / "ran"
        binary = f'#!/bin/sh\n: > "{marker}"\n'.encode()
        archive = self.crafted([("bin/spielorders", binary),
                                ("share/spielorders/agent-guidance/SKILL.md", b"# Guidance\n")],
                               checksum="0" * 64)
        result = self.install("--archive", str(archive))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match its checksum", result.stderr)
        self.assertFalse(marker.exists(), "nothing in a mismatched archive runs")
        self.untouched()

    def test_links_and_special_files_are_refused(self):
        for entry in [("bin/spielorders", "/bin/sh"), ("share/spielorders/agent-guidance/SKILL.md", "../x")]:
            entries = dict([("bin/spielorders", b"#!/bin/sh\n"),
                            ("share/spielorders/agent-guidance/SKILL.md", b"# Guidance\n")])
            entries[entry[0]] = entry[1]
            result = self.install("--archive", str(self.crafted(list(entries.items()))))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsupported entry", result.stderr)
            self.untouched()

    def test_an_invalid_receipt_is_refused_and_kept(self):
        self.prefix.mkdir()
        receipt = self.prefix / ".install.json"
        receipt.write_text("not a receipt\n")
        result = self.install("--archive", str(self.archive))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ownership record is invalid", result.stderr)
        self.assertEqual(receipt.read_text(), "not a receipt\n")
        self.assertFalse((self.prefix / "bin/spielorders").exists())

    def test_a_release_tag_must_be_a_plain_word(self):
        result = self.install("--version", "v1;touch x")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported release tag", result.stderr)
        self.untouched()

    def test_an_empty_path_gains_only_the_installation(self):
        result = self.install("--archive", str(self.archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        found = subprocess.run(["/usr/bin/env", "-i", "PATH=", "/bin/sh", "-c",
                                f'. "{self.prefix}/env"; printf %s "$PATH"'],
                               capture_output=True, text=True, check=True)
        self.assertEqual(found.stdout, str(self.prefix / "bin"))

    def test_a_profile_that_never_returns_times_out(self):
        profile = self.profiles / ".zshrc"
        child = self.base / "profile-child"
        original = f'sleep 45 &\nprintf "%s\\n" "$!" > "{child}"\nwait\n'.encode()
        profile.write_bytes(original)
        started = time.monotonic()
        result = self.install("--archive", str(self.archive))
        self.assertLess(time.monotonic() - started, 44)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("did not finish within 30 seconds", result.stderr)
        self.assertEqual(profile.read_bytes(), original)
        self.assertFalse((self.prefix / "bin/spielorders").exists())
        pid = int(child.read_text())
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            pass
        else:
            os.kill(pid, signal.SIGKILL)
            self.fail("The timed-out profile left its child running")

    def test_a_forced_kill_is_put_back_by_the_next_installation(self):
        profile = self.profiles / ".zshrc"
        ready = self.base / "shell-ready"
        pipe = self.base / "shell-input"
        os.mkfifo(pipe)
        original = f'[ -e "{ready}" ] || {{ : > "{ready}"; read -r < "{pipe}"; }}\n'.encode()
        profile.write_bytes(original)
        process = subprocess.Popen(["sh", str(RELEASE_INSTALLER), "--prefix", str(self.prefix),
                                    "--archive", str(self.archive)],
                                   env=self.environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 20
            while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(ready.exists(), "The fresh shell must reach the owner's profile")
            os.kill(process.pid, signal.SIGKILL)
            process.communicate()
        finally:
            try:
                descriptor = os.open(pipe, os.O_WRONLY | os.O_NONBLOCK)
                try:
                    os.write(descriptor, b"continue\n")
                finally:
                    os.close(descriptor)
            except OSError:
                pass
        self.assertTrue(profile.read_bytes() != original, "the kill left the profile published")
        (self.prefix / ".install.lock").rmdir()

        result = self.install("--archive", str(self.archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(profile.read_bytes().startswith(original))
        self.assertEqual(profile.read_bytes().count(b"spielorders initialize >>>"), 1)
        self.assertEqual([path.name for path in self.prefix.iterdir() if path.name.startswith(".install-")], [])
        self.assertEqual([path.name for path in self.profiles.iterdir() if path.name.startswith(".spielorders-")], [])


if __name__ == "__main__":
    unittest.main()
