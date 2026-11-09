#!/usr/bin/env bash
# The CLI's own native services, seen once by hand: its Spotlight query against the real
# index, and its Trash move against the real Trash.
#
# Not reachable from scripts/gate.sh and never run unattended. Spotlight indexes a
# temporary directory late or never, and the Trash is the owner's, so the gate pins the
# decided rules against supplied hits and a sandbox that refuses the Trash, and this is
# where the real services are watched.
set -euo pipefail

cd "$(dirname "$0")/.."

PACKAGE="$HOME/SpielOrders-smoke-$(date +%Y%m%d-%H%M%S)"
WORD="zirconium"

cat <<EXPECTED
This builds a Package at
  $PACKAGE
and asks the real index for "$WORD". The directory does not exist yet and this script
creates it; it deletes nothing, and it takes no argument, so there is no path it can be
pointed at by mistake. It leaves the Package behind — remove it when you are done:
  rm -rf "$PACKAGE"

A correct run prints, in order:

  1. FOUND, then one Order group holding exactly two files, in this order, each spelled
     the way the Package spells it:
       Orders/<the Order>/vendor-pack/README.md    Kind md,  Modified <a local date and time>
       Orders/<the Order>/notes-$WORD.txt     Kind txt, Modified <a local date and time>
     Filename order decides — README before notes, because a capital R sorts before a
     lowercase n — since mdfind offers no relevance and no rank is reported.

     Two files, not three. The README here is the one inside vendor-pack: a familiar name
     in somebody else's folder is still their file. The Order's own README.md carries the
     same word and Spotlight does index it, so its absence is the managed-document
     exclusion working against the real index. The reserved .spielorders subtree is not
     planted with anything, because Spotlight does not index a dotted directory at all and
     a run here could say nothing about that exclusion; the test suite proves it instead.

  2. TRASHED — two ignored tests run by name. One asks the shipped mdutil for the state
     of the volume a temporary directory is on. The other runs `order remove --delete`
     on a disposable Package in your temporary directory, checks the whole tree arrived
     in your Trash, and takes its own directory back out of the Trash. Nothing else in
     your Trash is touched.

If it prints NOT INDEXED, Spotlight has not reached the Package. That is the most likely
first outcome, it is not a failure of SpielOrders, and nothing after it has run: give it a
minute and run this again. An empty answer from SpielOrders is reported as EMPTY and is a
failure, never a pass.

EXPECTED

read -r -p "Press return to build the Package and ask the index. " _ || true

cargo build --quiet
CLI="target/debug/spielorders"

mkdir -p "$PACKAGE"
"$CLI" init --root "$PACKAGE" >/dev/null
# A non-ASCII title, because Spotlight reports the composed name the library stores in
# decomposed form, and only the real index shows the search matching it anyway.
"$CLI" add-order "Café lasers" --root "$PACKAGE" >/dev/null
ORDER="$PACKAGE/Orders/$(ls "$PACKAGE/Orders" | head -1)"

# The file the index should find; the Order's own README, which is managed and must not
# come back; and a nested README wearing a managed name, which is somebody else's file and
# must. Nothing is planted under .spielorders: Spotlight does not index it, so a file there
# would prove nothing and would raise a reserved-content warning this run cannot explain.
FOUND_FILE="$ORDER/notes-$WORD.txt"
printf 'A datasheet mentioning %s.\n' "$WORD" > "$FOUND_FILE"
printf '\n# Notes\n\nThis README mentions %s.\n' "$WORD" >> "$ORDER/README.md"
mkdir -p "$ORDER/vendor-pack"
printf '# Vendor pack\n\nAlso mentions %s.\n' "$WORD" > "$ORDER/vendor-pack/README.md"

# Both files, not the first: the plain filename match arrives long before the one that
# matches on content, so stopping at one hit would show a single file where two are
# expected and make index lag look like a broken exclusion.
echo "Waiting for the index to reach both files (up to 120s)..."
for _ in $(seq 1 60); do
  indexed="$(mdfind -onlyin "$PACKAGE" "$WORD" 2>/dev/null | wc -l | tr -d ' ')"
  [ "$indexed" -ge 3 ] && break
  sleep 2
done

if [ "$indexed" -lt 3 ] 2>/dev/null; then
  echo
  echo "NOT INDEXED — Spotlight has indexed $indexed of the 3 files that carry the word."
  echo "SpielOrders is not at fault and nothing below has run. The Package is left in place:"
  echo "  $PACKAGE"
  echo "Run this script again in a minute."
  exit 1
fi

echo
echo "FOUND — the index answered. What SpielOrders reports, with the file facts shown:"
echo
"$CLI" search supporting-material "$WORD" --root "$PACKAGE" --verbose

reported="$("$CLI" search supporting-material "$WORD" --root "$PACKAGE" --json \
  | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len((d["data"] or {}).get("files",[])))')"
if [ "$reported" -eq 0 ]; then
  echo
  echo "EMPTY — the index answered but SpielOrders reported no file. That is a failure, not"
  echo "a pass: the Package is left at $PACKAGE for you to look at."
  exit 1
fi
echo
echo "SpielOrders reported $reported file(s)."

echo
read -r -p "Press return to ask mdutil for a state and move a disposable Order to the Trash. " _ || true
cargo test --quiet -p spielorders-cli --bin spielorders -- --ignored \
  the_shipped_tools_get_a_state_for_the_volume_a_directory_is_on
cargo test --quiet -p spielorders-cli --test cli -- --ignored \
  deleting_moves_the_intact_removed_directory_to_the_platform_trash
echo "TRASHED — the disposable Order reached the Trash whole and was taken back out."

echo
echo "The Package is left at $PACKAGE. Remove it with: rm -rf \"$PACKAGE\""
