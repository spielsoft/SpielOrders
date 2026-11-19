#!/bin/sh
# Package a built CLI with the agent guidance from this checkout as one release archive.
#
#   scripts/package-cli.sh BINARY PLATFORM DIRECTORY
#
# Writes DIRECTORY/spielorders-PLATFORM.tar.gz, holding spielorders-PLATFORM/ with
# bin/spielorders and share/spielorders/agent-guidance, and its SHA-256 beside it as
# spielorders-PLATFORM.tar.gz.sha256, the pair scripts/install.sh installs.

set -eu
[ $# -eq 3 ] || { echo "usage: $0 BINARY PLATFORM DIRECTORY" >&2; exit 2; }
binary=$1
platform=$2
directory=$3
root=$(cd "$(dirname "$0")/.." && pwd -P)
name=spielorders-$platform
work=$(mktemp -d "${TMPDIR:-/tmp}/spielorders-package.XXXXXX")
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/$name/bin" "$work/$name/share/spielorders" "$directory"
cp "$binary" "$work/$name/bin/spielorders"
chmod 755 "$work/$name/bin/spielorders"
cp -R "$root/crates/spielorders/agent-guidance" "$work/$name/share/spielorders/agent-guidance"
# macOS tar would otherwise add AppleDouble files that the installer would take for
# guidance.
COPYFILE_DISABLE=1 tar -czf "$directory/$name.tar.gz" -C "$work" "$name"
(
  cd "$directory"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$name.tar.gz" >"$name.tar.gz.sha256"
  else
    shasum -a 256 "$name.tar.gz" >"$name.tar.gz.sha256"
  fi
)
