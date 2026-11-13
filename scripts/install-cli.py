#!/usr/bin/env python3
"""Install the standalone development CLI and register it with the user's zsh."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import stat
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
GUIDANCE = Path("share/spielorders/agent-guidance")
RECEIPT = Path(".install.json")
BEGIN = b"# >>> spielorders initialize >>>"
END = b"# <<< spielorders initialize <<<"
# Once rollback or cleanup starts, an interruption must not abandon the backups it restores.
settling = False


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def owned(name):
    path = Path(name)
    return (not path.is_absolute() and ".." not in path.parts
            and (name in ("bin/spielorders", "env") or path.is_relative_to(GUIDANCE)))


def target(prefix, name):
    path = prefix / name
    for parent in [path, *path.parents]:
        if parent == prefix.parent:
            break
        if parent.is_symlink():
            raise ValueError(f"Refusing linked installation path: {parent}")
        if parent != path and parent.exists() and not parent.is_dir():
            raise ValueError(f"Expected an installation directory: {parent}")
    if path.exists() and not path.is_file():
        raise ValueError(f"Expected an ordinary installation file: {path}")
    return path


def profiles(prefix):
    probe = subprocess.run(["/bin/zsh", "-c", 'printf "\\0%s\\0" "${ZDOTDIR:-$HOME}"'],
                           capture_output=True, check=True)
    directory = Path(os.fsdecode(probe.stdout.split(b"\0")[-2]))
    if not directory.is_absolute():
        raise ValueError("Relative ZDOTDIR is unsupported; use an absolute shell configuration directory")
    line = f"[ -r {shlex.quote(str(prefix / 'env'))} ] && . {shlex.quote(str(prefix / 'env'))}\n".encode()
    block = BEGIN + b"\n# !! Contents within this block are managed by SpielOrders !!\n" + line + END + b"\n"
    result = []
    seen = set()
    for name in (".zprofile", ".zshrc"):
        path = directory / name
        if path.is_symlink():
            path = path.resolve(strict=True)
        path = path.absolute()
        if path in seen:
            continue
        seen.add(path)
        if path.exists() and not path.is_file():
            raise ValueError(f"Expected an ordinary shell profile: {path}")
        if path.exists() and (not os.access(path, os.W_OK) or path.stat().st_uid != os.geteuid() or path.stat().st_nlink != 1):
            raise ValueError(f"Shell profile is not safely replaceable by this user: {path}")
        before = path.read_bytes() if path.exists() else None
        lines = (before or b"").splitlines(keepends=True)
        starts = [index for index, text in enumerate(lines) if text.rstrip(b"\r\n") == BEGIN]
        ends = [index for index, text in enumerate(lines) if text.rstrip(b"\r\n") == END]
        if not starts and not ends:
            after = (before or b"") + (b"\n" if before and not before.endswith(b"\n") else b"") + block
        elif len(starts) == len(ends) == 1 and starts[0] < ends[0]:
            after = b"".join(lines[:starts[0]]) + block + b"".join(lines[ends[0] + 1:])
        else:
            raise ValueError(f"Malformed SpielOrders block; profile left unchanged: {path}")
        if before != after:
            result.append((path, before, after))
    return result


def install(prefix):
    global settling
    if prefix.is_symlink():
        raise ValueError("The installation prefix must not be a link")
    prefix = prefix.resolve()
    if any(character in str(prefix) for character in "\n\r\0:"):
        raise ValueError("The installation prefix cannot contain a line break, NUL, or PATH separator")
    if prefix in (Path("/"), Path.home().resolve(), ROOT):
        raise ValueError("Choose a dedicated installation directory")
    if prefix.exists() and not prefix.is_dir():
        raise ValueError("The installation prefix must be a directory")
    changes = profiles(prefix)
    subprocess.run(["cargo", "build", "-q", "-p", "spielorders-cli", "--release", "--locked"],
                   cwd=ROOT, check=True)
    metadata = subprocess.run(["cargo", "metadata", "-q", "--format-version", "1", "--no-deps", "--locked"],
                              cwd=ROOT, check=True, capture_output=True)
    built = Path(json.loads(metadata.stdout)["target_directory"]) / "release/spielorders"
    prefix.mkdir(parents=True, exist_ok=True)
    lock = prefix / ".install.lock"
    try:
        lock.mkdir()
    except FileExistsError:
        raise ValueError(f"Another installation holds {lock}, or an interrupted one left it behind. "
                         "If no installer is running, remove that directory and install again.") from None
    stage = None
    installed = False
    rolled_back = False
    published = []
    profile_stages = []
    try:
        stage = Path(tempfile.mkdtemp(prefix=".install-", dir=prefix))
        receipt = target(prefix, RECEIPT)
        receipt_bytes = receipt.read_bytes() if receipt.exists() else None
        previous = json.loads(receipt_bytes) if receipt_bytes is not None else {}
        if not isinstance(previous, dict) or not all(
                isinstance(name, str) and owned(name) and isinstance(value, str)
                for name, value in previous.items()):
            raise ValueError("The installation ownership record is invalid")
        payload = stage / "payload"
        (payload / "bin").mkdir(parents=True)
        shutil.copy2(built, payload / "bin/spielorders")
        (payload / "bin/spielorders").chmod(0o755)
        shutil.copytree(ROOT / "crates/spielorders/agent-guidance", payload / GUIDANCE, symlinks=True)
        binary = shlex.quote(str(prefix / "bin"))
        (payload / "env").write_text(
            '_spielorders_path() {\n'
            f'  local result={binary} remaining="$PATH" entry more\n'
            '  while :; do\n'
            '    case "$remaining" in\n'
            '      *:*) entry=${remaining%%:*}; remaining=${remaining#*:}; more=1 ;;\n'
            '      *) entry=$remaining; more=0 ;;\n'
            '    esac\n'
            f'    [ "$entry" = {binary} ] || result="$result:$entry"\n'
            '    [ "$more" = 1 ] || break\n'
            '  done\n'
            '  export PATH="$result"\n'
            '}\n_spielorders_path\nunset -f _spielorders_path\n')
        files = {}
        for path in payload.rglob("*"):
            if path.is_symlink() or not (path.is_dir() or path.is_file()):
                raise ValueError(f"Unsupported payload entry: {path}")
            if path.is_file():
                files[str(path.relative_to(payload))] = digest(path)
        subprocess.run([str(payload / "bin/spielorders"), "--version"], check=True, capture_output=True)
        subprocess.run([str(payload / "bin/spielorders"), "agent", "defaults", "--quiet"],
                       check=True, capture_output=True)
        for name in set(previous) | set(files):
            path = target(prefix, name)
            if path.exists() and (name not in previous or digest(path) != previous[name]):
                raise ValueError(f"Installation conflict; existing content is preserved: {path}")
        (payload / RECEIPT).write_text(json.dumps(files, sort_keys=True, indent=2) + "\n")
        for name in sorted(set(previous) | set(files)) + [str(RECEIPT)]:
            path = target(prefix, name)
            backup = stage / "backup" / name if path.exists() else None
            source = payload / name
            source = source if source.exists() else None
            expected_new = digest(source) if source is not None else None
            published.append((path, backup, source, expected_new))
            if path.exists():
                expected = hashlib.sha256(receipt_bytes).hexdigest() if name == str(RECEIPT) and receipt_bytes is not None else previous.get(name)
                if digest(path) != expected:
                    raise ValueError(f"Installation changed during update: {path}")
                backup.parent.mkdir(parents=True, exist_ok=True)
                os.replace(path, backup)
                if digest(backup) != expected:
                    raise ValueError(f"Installation changed while publishing: {path}")
            if source is not None:
                path.parent.mkdir(parents=True, exist_ok=True)
                os.link(source, path)  # Never replace a concurrent writer's new file.
        for path, before, after in changes:
            path.parent.mkdir(parents=True, exist_ok=True)
            local = Path(tempfile.mkdtemp(prefix=".spielorders-", dir=path.parent))
            profile_stages.append(local)
            source = local / "after"
            with source.open("x+b") as stream:
                if before is not None:
                    subprocess.run(["/bin/cp", "-p", str(path), str(source)], check=True)
                    original = path.stat()
                    copied = source.stat()
                    if (os.fstat(stream.fileno()).st_ino != copied.st_ino
                            or (copied.st_uid, copied.st_gid, stat.S_IMODE(copied.st_mode))
                            != (original.st_uid, original.st_gid, stat.S_IMODE(original.st_mode))):
                        raise ValueError(f"Cannot preserve shell profile metadata: {path}")
                stream.write(after)
                stream.truncate()
                stream.flush()
                os.fsync(stream.fileno())
            if path.is_symlink() or (path.read_bytes() if path.exists() else None) != before:
                raise ValueError(f"Shell profile changed during installation: {path}")
            backup = local / "before" if before is not None else None
            published.append((path, backup, source, digest(source)))
            if backup is not None:
                os.replace(path, backup)
                if backup.read_bytes() != before:
                    raise ValueError(f"Shell profile changed while publishing: {path}")
            os.link(source, path)
        # A new login shell starts from the profiles, not from this installer's PATH.
        fresh = {name: os.environ[name] for name in ("HOME", "USER", "LOGNAME", "SHELL", "TERM", "ZDOTDIR")
                 if name in os.environ}
        verified = subprocess.run(["/bin/zsh", "-lic", "command -v spielorders"],
                                  env=fresh, capture_output=True, text=True)
        if verified.stdout.strip().splitlines()[-1:] != [str(prefix / "bin/spielorders")]:
            raise ValueError("A fresh zsh does not select the installed CLI; the installation was rolled back")
        for name, expected in files.items():
            if digest(target(prefix, name)) != expected:
                raise ValueError(f"Installed content changed before verification: {prefix / name}")
        subprocess.run([str(prefix / "bin/spielorders"), "--version"], check=True, capture_output=True)
        subprocess.run([str(prefix / "bin/spielorders"), "agent", "defaults", "--quiet"],
                       check=True, capture_output=True)
        installed = True
    except BaseException as failure:
        settling = True
        problems = []
        for path, backup, source, expected in reversed(published):
            try:
                if backup is not None and not backup.exists():
                    continue  # The old file was not moved; leave it untouched.
                if path.exists() or path.is_symlink():
                    if path.is_symlink() or source is None or not path.samefile(source) or digest(path) != expected:
                        raise ValueError("path changed again; not replacing it")
                    path.unlink()
                if backup is not None:
                    os.link(backup, path)
            except Exception as error:
                problems.append(f"{path}: {error}")
        if problems:
            locations = ", ".join(str(path) for path in [stage, *profile_stages] if path is not None)
            raise RuntimeError(f"{failure}\nRollback needs attention; backups kept at {locations}:\n"
                               + "\n".join(problems)) from failure
        rolled_back = True
        raise
    finally:
        settling = True
        # Backups are discarded only after success or a completed rollback.
        if stage is not None and (installed or rolled_back):
            shutil.rmtree(stage)
            for directory in profile_stages:
                shutil.rmtree(directory)
        lock.rmdir()
        settling = False
    print(f"Installed {prefix / 'bin/spielorders'}. Start a new shell to use spielorders on PATH.")


if __name__ == "__main__":
    def interrupted(_number, _frame):
        if not settling:
            raise RuntimeError("Installation interrupted")

    signal.signal(signal.SIGINT, interrupted)
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".spielorders",
                        help="installation directory (default: ~/.spielorders)")
    arguments = parser.parse_args()
    try:
        install(arguments.prefix.expanduser())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"spielorders installation failed: {error}\n")
