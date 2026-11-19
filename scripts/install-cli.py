#!/usr/bin/env python3
"""Build the CLI from this checkout and install it as a release would be installed.

The build is packaged exactly as the release workflow packages one, and the public
installer, scripts/install.sh, installs that archive: one installer, whether the archive
was downloaded or built here.
"""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".spielorders",
                        help="installation directory (default: ~/.spielorders)")
    prefix = parser.parse_args().prefix.expanduser()
    if not prefix.is_symlink() and prefix.resolve() == ROOT:
        parser.exit(1, "spielorders installation failed: Choose a dedicated installation directory\n")
    subprocess.run(["cargo", "build", "-q", "-p", "spielorders-cli", "--release", "--locked"],
                   cwd=ROOT, check=True)
    metadata = subprocess.run(["cargo", "metadata", "-q", "--format-version", "1", "--no-deps", "--locked"],
                              cwd=ROOT, check=True, capture_output=True)
    built = Path(json.loads(metadata.stdout)["target_directory"]) / "release/spielorders"
    with tempfile.TemporaryDirectory(prefix="spielorders-development-") as directory:
        subprocess.run(["sh", str(ROOT / "scripts/package-cli.sh"), str(built), "development", directory],
                       check=True)
        installer = subprocess.Popen(
            ["sh", str(ROOT / "scripts/install.sh"), "--prefix", str(prefix),
             "--archive", str(Path(directory) / "spielorders-development.tar.gz")])

        # An interruption is the installer's to settle: it rolls back what it published.
        def forward(number, _frame):
            installer.send_signal(number)

        for number in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(number, forward)
        return installer.wait()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except subprocess.CalledProcessError as error:
        sys.exit(f"spielorders installation failed: {error}")
