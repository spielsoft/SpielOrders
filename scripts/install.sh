#!/bin/sh
# Install the SpielOrders command-line tool and its agent guidance.
#
#   curl -fsSL https://github.com/spielsoft/SpielOrders/releases/latest/download/install.sh | sh
#   curl -fsSL https://github.com/spielsoft/SpielOrders/releases/latest/download/install.sh | sh -s -- --version v0.1.0
#   sh install.sh --archive spielorders-macos-arm64.tar.gz
#
# Options:
#   --version TAG    install that release instead of the latest one
#   --prefix DIR     installation directory (default: ~/.spielorders)
#   --archive FILE   install this release archive, checked against FILE.sha256 beside it,
#                    instead of downloading one
#
# The archive's checksum is verified before anything in it is extracted or run. The
# complete installation is staged inside the prefix, published file by file without
# replacing anything the installer does not own, and registered on PATH for the login
# shell (zsh or bash). A fresh login shell must then select the installed CLI; otherwise,
# or on any failure or interruption the installer can catch, everything published is put
# back as it was. A forced kill can leave an installation half-published; the next run
# puts that one back before installing. An unchanged earlier installation is updated in
# place. Downloads use plain HTTPS.

# The whole script is one group, so a download cut short anywhere fails to parse and
# runs nothing.
{

REPOSITORY=https://github.com/spielsoft/SpielOrders
GUIDANCE=share/spielorders/agent-guidance
RECEIPT=.install.json
BEGIN='# >>> spielorders initialize >>>'
END='# <<< spielorders initialize <<<'
NEWLINE='
'
CR=$(printf '\r')

# Everything the cleanup reads, set before anything can fail.
created=
work=
stage=
lock=
locals=
verifying=
watchdog=
installed=
journal_count=0

fail() {
  printf 'spielorders installation failed: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: install.sh [--version TAG] [--prefix DIR] [--archive FILE]

Installs the SpielOrders CLI and its agent guidance under DIR (default ~/.spielorders)
and registers it on PATH for the login shell (zsh or bash). Without --version, the
latest release is downloaded; --archive installs a release archive already on disk,
checked against FILE.sha256 beside it.
USAGE
}

# The SHA-256 of the file at $1, read from standard input so no file name is echoed.
digest() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum <"$1" | cut -d ' ' -f 1
  else
    shasum -a 256 <"$1" | cut -d ' ' -f 1
  fi
}

# $1 as a shell word, left bare where it needs no quoting.
quoted() {
  case $1 in
    '') printf "''" ;;
    *[!A-Za-z0-9@%+=:,./_-]*) printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\"'\"'/g")" ;;
    *) printf '%s' "$1" ;;
  esac
}

# The physical absolute spelling of $1, resolving the part of it that exists.
physical() {
  physical_path=$1
  case $physical_path in /*) ;; *) physical_path=$(pwd -P)/$physical_path ;; esac
  physical_rest=
  while [ ! -d "$physical_path" ]; do
    physical_rest=/${physical_path##*/}$physical_rest
    physical_path=${physical_path%/*}
    [ -n "$physical_path" ] || physical_path=/
  done
  physical_base=$(cd -P "$physical_path" && pwd -P && printf x) || return 1
  physical_base=${physical_base%"${NEWLINE}x"}
  [ "$physical_base" = / ] && physical_base=
  printf '%s' "$physical_base$physical_rest"
}

# The file a profile path names, following links, as a physical absolute path.
followed() {
  followed_path=$1
  followed_hops=0
  while [ -L "$followed_path" ]; do
    followed_hops=$((followed_hops + 1))
    [ "$followed_hops" -le 40 ] || return 1
    followed_target=$(readlink "$followed_path") || return 1
    case $followed_target in
      /*) followed_path=$followed_target ;;
      *) followed_path=${followed_path%/*}/$followed_target ;;
    esac
  done
  if [ -d "${followed_path%/*}" ]; then
    physical "$followed_path"
  else
    printf '%s' "$followed_path"
  fi
}

# Creates directory $1 and any missing parents, recording each one so that a rollback
# removes what this installation created.
made() {
  made_path=$1
  made_missing=
  while [ ! -e "$made_path" ] && [ ! -L "$made_path" ] && [ -n "$made_path" ]; do
    made_missing=$made_path$NEWLINE$made_missing
    made_path=${made_path%/*}
  done
  mkdir -p "$1" || return 1
  printf '%s' "$made_missing" >>"$created"
}

# Removes, innermost first, every directory this installation created and left empty.
unmade() {
  [ -n "$created" ] && [ -f "$created" ] || return 0
  awk '{ line[NR] = $0 } END { for (i = NR; i > 0; i--) print line[i] }' "$created" |
    while IFS= read -r unmade_path; do
      [ -n "$unmade_path" ] && rmdir "$unmade_path" 2>/dev/null
    done
}

# Whether $1 is a name an installation may own.
owned() {
  case $1 in
    /* | *..* | *[!A-Za-z0-9._/-]*) return 1 ;;
    bin/spielorders | env) return 0 ;;
    "$GUIDANCE"/?*) return 0 ;;
  esac
  return 1
}

# Refuses an installation path that is reached through a link, sits under something that
# is not a directory, or is itself anything but an ordinary file.
checked() {
  checked_rest=$1
  checked_path=$prefix
  while :; do
    case $checked_rest in
      */*) checked_part=${checked_rest%%/*}; checked_rest=${checked_rest#*/} ;;
      *) checked_part=$checked_rest; checked_rest= ;;
    esac
    checked_path=$checked_path/$checked_part
    [ -L "$checked_path" ] && fail "Refusing linked installation path: $checked_path"
    if [ -n "$checked_rest" ]; then
      if [ -e "$checked_path" ] && [ ! -d "$checked_path" ]; then
        fail "Expected an installation directory: $checked_path"
      fi
    else
      break
    fi
  done
  if [ -e "$checked_path" ] && [ ! -f "$checked_path" ]; then
    fail "Expected an ordinary installation file: $checked_path"
  fi
}

# The hash `$2` records for name `$1`, or nothing.
recorded() {
  awk -v name="$1" '$2 == name { print $1; exit }' "$2"
}

# The hash-and-name lines of an ownership record, or failure where it is not one.
receipt_lines() {
  awk '
    NR == 1 && $0 == "{}" { closed = 1; next }
    NR == 1 { if ($0 != "{") bad = 1; next }
    closed { bad = 1; next }
    $0 == "}" { closed = 1; next }
    {
      if ($0 !~ /^  "[^"\\]+": "[0-9a-f]+",?$/) { bad = 1; next }
      line = $0
      sub(/^  "/, "", line)
      name = line
      sub(/": ".*$/, "", name)
      hash = line
      sub(/^[^"]*": "/, "", hash)
      sub(/",?$/, "", hash)
      if (length(hash) != 64) { bad = 1; next }
      print hash " " name
    }
    END { if (bad || !closed) exit 1 }
  ' "$1"
}

# Appends one publication to the journal rollback reads backwards: the installed path,
# where its earlier content was moved (or nothing), what was linked there (or nothing),
# and that content's hash.
journal() {
  journal_count=$((journal_count + 1))
  journal_entry=$stage/journal/$(printf '%06d' "$journal_count")
  printf '%s\n%s\n%s\n%s\n' "$1" "$2" "$3" "$4" >"$journal_entry" ||
    fail "Cannot record the installation journal"
}

# Puts every publication journaled in stage $1 back, newest first. Prints what it could
# not restore.
roll_back() {
  roll_stage=$1
  [ -n "$roll_stage" ] && [ -d "$roll_stage/journal" ] || return 0
  for roll_entry in $(ls "$roll_stage/journal" | sort -r); do
    {
      IFS= read -r roll_path
      IFS= read -r roll_backup
      IFS= read -r roll_source
      IFS= read -r roll_hash
    } <"$roll_stage/journal/$roll_entry"
    if [ -n "$roll_backup" ] && [ ! -e "$roll_backup" ]; then
      continue # The earlier file was never set aside; leave it untouched.
    fi
    if [ -e "$roll_path" ] || [ -L "$roll_path" ]; then
      if [ -L "$roll_path" ] || [ -z "$roll_source" ] || [ ! "$roll_path" -ef "$roll_source" ] ||
        [ "$(digest "$roll_path")" != "$roll_hash" ]; then
        # Still the earlier file where publishing never replaced it.
        [ -n "$roll_backup" ] && [ "$roll_path" -ef "$roll_backup" ] && continue
        printf '%s: changed again; not replacing it\n' "$roll_path"
        continue
      fi
      if [ -n "$roll_backup" ]; then
        # The earlier file returns in one rename, so the path is never missing.
        # Its temporary name stays in an exclusively created directory beside the
        # backup, on the same filesystem, never at an unowned name beside the target.
        roll_restore=$(mktemp -d "${roll_backup%/*}/restore.XXXXXX") &&
          ln "$roll_backup" "$roll_restore/file" && mv -f "$roll_restore/file" "$roll_path" &&
          rmdir "$roll_restore" ||
          printf '%s: cannot restore\n' "$roll_path"
        continue
      fi
      rm -f "$roll_path" || { printf '%s: cannot remove\n' "$roll_path"; continue; }
    elif [ -n "$roll_backup" ]; then
      ln "$roll_backup" "$roll_path" || printf '%s: cannot restore\n' "$roll_path"
    fi
  done
}

# The staging directories beside shell profiles that stage $1 journaled.
profile_stages() {
  [ -d "$1/journal" ] || return 0
  for profile_entry in $(ls "$1/journal"); do
    sed -n 3p "$1/journal/$profile_entry"
  done | while IFS= read -r profile_source; do
    profile_stage=${profile_source%/*}
    case ${profile_stage##*/} in .spielorders-??????) printf '%s\n' "$profile_stage" ;; esac
  done
}

# Puts back an installation a forced kill left half-published in this prefix, or refuses
# and names what needs attention.
recover() {
  set +f
  for recover_stage in "$prefix"/.install-??????; do
    set -f
    [ -d "$recover_stage" ] && [ ! -L "$recover_stage" ] || continue
    # A matching name alone grants no ownership of a directory in the prefix.
    # Every transaction records its physical prefix before it can publish anything.
    [ -f "$recover_stage/root" ] && [ ! -L "$recover_stage/root" ] &&
      [ "$(cat "$recover_stage/root")" = "$prefix" ] || continue
    recover_problems=$(roll_back "$recover_stage")
    if [ -n "$recover_problems" ]; then
      fail "An interrupted installation left $recover_stage, and putting it back needs attention:$NEWLINE$recover_problems${NEWLINE}Its journal names each file and the backup it set aside. Restore those by hand, remove that directory, and install again."
    fi
    profile_stages "$recover_stage" | while IFS= read -r recover_local; do
      rm -rf "$recover_local"
    done
    rm -rf "$recover_stage" || fail "Cannot remove the interrupted installation's stage: $recover_stage"
  done
  set -f
}

# Stops only the verification shell and the processes it is still waiting for. Freeze
# the shell first so that stopping a child cannot let the profile continue and publish
# the success marker. Descendants are selected by parent PID, never by command name.
stop_verification() {
  [ -n "$verifying" ] || return 0
  kill -STOP "$verifying" 2>/dev/null || return 0
  verification_children=$(ps -e -o pid= -o ppid= | awk -v root="$verifying" '
    { parent[$1] = $2 }
    function descendants(root, child) {
      for (child in parent) if (parent[child] == root) {
        descendants(child)
        print child
      }
    }
    END { descendants(root) }
  ')
  for verification_child in $verification_children; do
    kill -KILL "$verification_child" 2>/dev/null
  done
  kill -KILL "$verifying" 2>/dev/null
}

settle_watchdog() {
  [ -n "$watchdog" ] || return 0
  kill "$watchdog" 2>/dev/null
  wait "$watchdog" 2>/dev/null
  watchdog=
}

cleanup() {
  cleanup_status=$?
  # Once settling starts, an interruption must not abandon the backups it restores.
  trap '' INT TERM HUP
  if [ -n "$verifying" ]; then
    stop_verification
    wait "$verifying" 2>/dev/null
  fi
  settle_watchdog
  settled=
  if [ -n "$installed" ]; then
    settled=1
  else
    problems=$(roll_back "$stage")
    if [ -n "$problems" ]; then
      printf 'Rollback needs attention; backups kept at:\n%s\n%s\n%s\n' \
        "$stage" "${locals%"$NEWLINE"}" "$problems" >&2
    else
      settled=1
    fi
  fi
  # Backups are discarded only after success or a completed rollback.
  if [ -n "$settled" ]; then
    [ -n "$stage" ] && rm -rf "$stage"
    while IFS= read -r cleanup_local; do
      [ -n "$cleanup_local" ] && rm -rf "$cleanup_local"
    done <<LOCALS
$locals
LOCALS
  fi
  [ -n "$lock" ] && rmdir "$lock" 2>/dev/null
  if [ -z "$installed" ] && [ -n "$settled" ]; then unmade; fi
  [ -n "$work" ] && rm -rf "$work"
  [ "$cleanup_status" -ne 0 ] || [ -n "$installed" ] || cleanup_status=1
  exit "$cleanup_status"
}

# The release platform this machine runs.
platform() {
  platform_system=$(uname -s)
  platform_machine=$(uname -m)
  # A Rosetta shell on Apple silicon reports x86_64; the machine still runs arm64.
  if [ "$platform_system" = Darwin ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then
    platform_machine=arm64
  fi
  case "$platform_system $platform_machine" in
    'Darwin arm64') printf macos-arm64 ;;
    'Linux x86_64' | 'Linux amd64') printf linux-x86_64 ;;
    'Linux aarch64' | 'Linux arm64') printf linux-arm64 ;;
    *) fail "Unsupported platform: $platform_system $platform_machine. Releases are built for macOS on Apple silicon and Linux on x86-64 and arm64." ;;
  esac
}

# Plans the managed PATH block in each profile the login shell reads, writing nothing
# outside the work directory: each changed profile's path, its earlier bytes, and its new
# bytes.
plan_profiles() {
  shell_name=${SHELL##*/}
  case $shell_name in
    zsh)
      zsh_directory=$("$SHELL" -c 'printf "\n@@spielorders@@%s" "${ZDOTDIR:-$HOME}"' </dev/null 2>/dev/null) ||
        fail "Cannot ask zsh for its configuration directory"
      zsh_directory=${zsh_directory##*@@spielorders@@}
      case $zsh_directory in
        /*) ;;
        *) fail "Relative ZDOTDIR is unsupported; use an absolute shell configuration directory" ;;
      esac
      candidates="$zsh_directory/.zprofile$NEWLINE$zsh_directory/.zshrc"
      ;;
    bash)
      # A login bash reads the first of these that exists; creating an earlier one would
      # hide the owner's later one.
      login=$HOME/.bash_profile
      for bash_login in .bash_profile .bash_login .profile; do
        if [ -e "$HOME/$bash_login" ] || [ -L "$HOME/$bash_login" ]; then
          login=$HOME/$bash_login
          break
        fi
      done
      candidates="$login$NEWLINE$HOME/.bashrc"
      ;;
    *) fail "Unsupported login shell: ${SHELL:-none}. The installer registers zsh and bash." ;;
  esac
  line="[ -r $(quoted "$prefix/env") ] && . $(quoted "$prefix/env")"
  block="$BEGIN$NEWLINE# !! Contents within this block are managed by SpielOrders !!$NEWLINE$line$NEWLINE$END"
  : >"$work/profiles"
  profile_count=0
  seen=
  while IFS= read -r candidate; do
    profile=$(followed "$candidate") || fail "Cannot follow shell profile link: $candidate"
    case $profile in *"$NEWLINE"* | *"$CR"*) fail "Unsupported shell profile path: $profile" ;; esac
    case "$NEWLINE$seen" in *"$NEWLINE$profile$NEWLINE"*) continue ;; esac
    seen=$seen$profile$NEWLINE
    if [ -e "$profile" ] && [ ! -f "$profile" ]; then
      fail "Expected an ordinary shell profile: $profile"
    fi
    profile_count=$((profile_count + 1))
    before=$work/profile.$profile_count.before
    after=$work/profile.$profile_count.after
    if [ -e "$profile" ]; then
      set -- $(ls -ldn "$profile")
      if [ ! -w "$profile" ] || [ "$3" != "$(id -u)" ] || [ "$2" != 1 ]; then
        fail "Shell profile is not safely replaceable by this user: $profile"
      fi
      cp "$profile" "$before" || fail "Cannot read shell profile: $profile"
      set -- $(awk -v b="$BEGIN" -v e="$END" '
        { sub(/\r$/, "") }
        $0 == b { s++; if (!fs) fs = NR }
        $0 == e { n++; if (!fe) fe = NR }
        END { print s + 0, n + 0, fs + 0, fe + 0 }' "$before")
      if [ "$1" = 0 ] && [ "$2" = 0 ]; then
        {
          cat "$before" &&
            if [ -s "$before" ] && [ "$(tail -c 1 "$before" | wc -l)" -eq 0 ]; then printf '\n'; fi &&
            printf '%s\n' "$block"
        } >"$after" || fail "Cannot plan the shell profile: $profile"
      elif [ "$1" = 1 ] && [ "$2" = 1 ] && [ "$3" -lt "$4" ]; then
        {
          { [ "$3" -le 1 ] || head -n $(($3 - 1)) "$before"; } &&
            printf '%s\n' "$block" &&
            tail -n +$(($4 + 1)) "$before"
        } >"$after" || fail "Cannot plan the shell profile: $profile"
      else
        fail "Malformed SpielOrders block; profile left unchanged: $profile"
      fi
      cmp -s "$before" "$after" && continue
    else
      rm -f "$before"
      printf '%s\n' "$block" >"$after" || fail "Cannot plan the shell profile: $profile"
    fi
    printf '%s\n' "$profile_count $profile" >>"$work/profiles"
  done <<EOF
$candidates
EOF
}

# The verified archive for this installation, extracted into the work directory.
obtain() {
  if [ -n "$archive" ]; then
    [ -f "$archive" ] || fail "No release archive at $archive"
    [ -f "$archive.sha256" ] || fail "No checksum beside the release archive: $archive.sha256"
    cp "$archive" "$work/archive.tar.gz" && cp "$archive.sha256" "$work/archive.sha256" ||
      fail "Cannot read the release archive"
  else
    command -v curl >/dev/null 2>&1 || fail "Downloading a release needs curl"
    asset=spielorders-$(platform).tar.gz || exit 1
    if [ -n "$version" ]; then
      base=$REPOSITORY/releases/download/$version
    else
      base=$REPOSITORY/releases/latest/download
    fi
    for download in "$asset:archive.tar.gz" "$asset.sha256:archive.sha256"; do
      curl -fsSL --proto '=https' --tlsv1.2 -o "$work/${download#*:}" "$base/${download%%:*}" </dev/null ||
        fail "Cannot download $base/${download%%:*}"
    done
  fi
  expected=$(cut -d ' ' -f 1 "$work/archive.sha256" | head -n 1)
  case $expected in
    *[!0-9a-f]*) fail "The release checksum file is malformed" ;;
  esac
  [ ${#expected} -eq 64 ] || fail "The release checksum file is malformed"
  [ "$(digest "$work/archive.tar.gz")" = "$expected" ] ||
    fail "The release archive does not match its checksum; nothing was installed"
  mkdir "$work/archive" && tar -xzf "$work/archive.tar.gz" -C "$work/archive" </dev/null ||
    fail "Cannot extract the release archive"
  set +f
  set -- "$work/archive"/*
  set -f
  [ $# -eq 1 ] && [ -d "$1" ] && [ ! -L "$1" ] || fail "The release archive has an unexpected layout"
  top=$1
  [ -z "$(find "$top" \( -type l -o \( ! -type f ! -type d \) -o -name "*$NEWLINE*" \) -print)" ] ||
    fail "The release archive holds an unsupported entry"
  (cd "$top" && find . -type f) | sed 's|^\./||' >"$work/archived" || fail "Cannot list the release archive"
  while IFS= read -r name; do
    { owned "$name" && [ "$name" != env ]; } || fail "The release archive holds an unexpected file: $name"
  done <"$work/archived"
  [ -f "$top/bin/spielorders" ] || fail "The release archive holds no spielorders executable"
  [ -f "$top/$GUIDANCE/SKILL.md" ] || fail "The release archive holds no agent guidance"
}

main() {
  # Nothing here relies on filename expansion except where it is turned on for one line.
  set -f
  version=
  prefix=$HOME/.spielorders
  archive=
  while [ $# -gt 0 ]; do
    case $1 in
      --version) [ $# -ge 2 ] || fail "--version needs a release tag"; version=$2; shift 2 ;;
      --prefix) [ $# -ge 2 ] || fail "--prefix needs a directory"; prefix=$2; shift 2 ;;
      --archive) [ $# -ge 2 ] || fail "--archive needs a file"; archive=$2; shift 2 ;;
      -h | --help) usage; exit 0 ;;
      *) fail "Unknown option: $1" ;;
    esac
  done
  [ -z "$version" ] || [ -z "$archive" ] || fail "--version and --archive name different releases; give one"
  case $version in *[!A-Za-z0-9._+-]*) fail "Unsupported release tag: $version" ;; esac

  trap cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP

  [ -n "${HOME:-}" ] || fail "HOME is not set"
  while [ "${prefix%/}" != "$prefix" ] && [ "$prefix" != / ]; do prefix=${prefix%/}; done
  [ -L "$prefix" ] && fail "The installation prefix must not be a link"
  case $prefix in
    *"$NEWLINE"* | *"$CR"* | *:*) fail "The installation prefix cannot contain a line break or PATH separator" ;;
  esac
  prefix=$(physical "$prefix") || fail "Cannot resolve the installation prefix"
  case "/$prefix/" in */./* | */../*) fail "Give the installation prefix without . or .. components" ;; esac
  home=$(physical "$HOME") || fail "Cannot resolve HOME"
  if [ "$prefix" = "" ] || [ "$prefix" = / ] || [ "$prefix" = "$home" ]; then
    fail "Choose a dedicated installation directory"
  fi
  [ ! -e "$prefix" ] || [ -d "$prefix" ] || fail "The installation prefix must be a directory"

  work=$(mktemp -d "${TMPDIR:-/tmp}/spielorders-install.XXXXXX") || fail "Cannot create a work directory"
  created=$work/created
  : >"$created"
  made "$prefix" || fail "Cannot create the installation prefix"
  if mkdir "$prefix/.install.lock" 2>/dev/null; then
    lock=$prefix/.install.lock
  else
    fail "Another installation holds $prefix/.install.lock, or an interrupted one left it behind. If no installer is running, remove that directory and install again."
  fi
  # Before the profiles are read: an interrupted installation may still hold them.
  recover
  plan_profiles
  obtain
  stage=$(mktemp -d "$prefix/.install-XXXXXX") || fail "Cannot create a staging directory"
  printf '%s\n' "$prefix" >"$stage/root" || fail "Cannot identify the staging directory"
  mkdir "$stage/journal" "$stage/payload" || fail "Cannot create a staging directory"

  checked "$RECEIPT"
  : >"$stage/previous"
  receipt_hash=
  if [ -e "$prefix/$RECEIPT" ]; then
    receipt_hash=$(digest "$prefix/$RECEIPT")
    receipt_lines "$prefix/$RECEIPT" >"$stage/previous" || fail "The installation ownership record is invalid"
    while read -r _ name; do
      owned "$name" || fail "The installation ownership record is invalid"
    done <"$stage/previous"
  fi

  payload=$stage/payload
  cp -R "$top/." "$payload/" || fail "Cannot stage the installation"
  chmod 755 "$payload/bin/spielorders" || fail "Cannot stage the installation"
  binary=$(quoted "$prefix/bin")
  cat >"$payload/env" <<EOF || fail "Cannot stage the installation"
_spielorders_path() {
  local result=$binary remaining="\$PATH" entry more
  # An empty PATH gains only this entry; an empty entry would name the current directory.
  [ -n "\$remaining" ] || { export PATH="\$result"; return; }
  while :; do
    case "\$remaining" in
      *:*) entry=\${remaining%%:*}; remaining=\${remaining#*:}; more=1 ;;
      *) entry=\$remaining; more=0 ;;
    esac
    [ "\$entry" = $binary ] || result="\$result:\$entry"
    [ "\$more" = 1 ] || break
  done
  export PATH="\$result"
}
_spielorders_path
unset -f _spielorders_path
EOF
  (cd "$payload" && find . -type f) | sed 's|^\./||' | LC_ALL=C sort >"$stage/names" ||
    fail "Cannot stage the installation"
  : >"$stage/files"
  while IFS= read -r name; do
    printf '%s %s\n' "$(digest "$payload/$name")" "$name" >>"$stage/files"
  done <"$stage/names"
  "$payload/bin/spielorders" --version </dev/null >/dev/null 2>&1 ||
    fail "The release's CLI does not run on this machine"
  "$payload/bin/spielorders" agent defaults --quiet </dev/null >/dev/null 2>&1 ||
    fail "The release's CLI does not find its agent guidance"

  { cut -d ' ' -f 2 "$stage/previous"; cat "$stage/names"; } | LC_ALL=C sort -u >"$stage/union"
  while IFS= read -r name; do
    checked "$name"
    if [ -e "$prefix/$name" ]; then
      earlier=$(recorded "$name" "$stage/previous")
      if [ -z "$earlier" ] || [ "$(digest "$prefix/$name")" != "$earlier" ]; then
        fail "Installation conflict; existing content is preserved: $prefix/$name"
      fi
    fi
  done <"$stage/union"

  {
    printf '{'
    separator=
    while read -r hash name; do
      printf '%s\n  "%s": "%s"' "$separator" "$name" "$hash"
      separator=,
    done <"$stage/files"
    if [ -n "$separator" ]; then printf '\n}\n'; else printf '}\n'; fi
  } >"$payload/$RECEIPT" || fail "Cannot stage the installation"

  printf '%s\n' "$RECEIPT" >>"$stage/union"
  while IFS= read -r name; do
    path=$prefix/$name
    checked "$name"
    backup=
    [ -e "$path" ] && backup=$stage/backup/$name
    source=
    fresh=
    if [ -e "$payload/$name" ]; then
      source=$payload/$name
      fresh=$(digest "$source")
    fi
    journal "$path" "$backup" "$source" "$fresh"
    if [ -n "$backup" ]; then
      if [ "$name" = "$RECEIPT" ]; then
        earlier=$receipt_hash
      else
        earlier=$(recorded "$name" "$stage/previous")
      fi
      [ "$(digest "$path")" = "$earlier" ] || fail "Installation changed during update: $path"
      # Set aside as a second link, so the path is never missing.
      mkdir -p "${backup%/*}" && ln "$path" "$backup" || fail "Cannot set aside $path"
      [ "$(digest "$backup")" = "$earlier" ] || fail "Installation changed while publishing: $path"
    fi
    if [ -n "$source" ] && [ -n "$backup" ]; then
      # The new file replaces the earlier one in one rename.
      ln "$source" "$stage/publishing" && mv -f "$stage/publishing" "$path" || fail "Cannot publish $path"
    elif [ -n "$source" ]; then
      # Linking never replaces a file another writer created meanwhile.
      made "${path%/*}" && ln "$source" "$path" || fail "Cannot publish $path"
    elif [ -n "$backup" ]; then
      rm -f "$path" || fail "Cannot retire $path"
    fi
  done <"$stage/union"

  while read -r number profile; do
    before=$work/profile.$number.before
    after=$work/profile.$number.after
    made "${profile%/*}" || fail "Cannot create the shell configuration directory for $profile"
    local_stage=$(mktemp -d "${profile%/*}/.spielorders-XXXXXX") || fail "Cannot stage $profile"
    locals=$locals$local_stage$NEWLINE
    source=$local_stage/after
    if [ -e "$before" ]; then
      cp -p "$profile" "$source" || fail "Cannot stage $profile"
      set -- $(ls -ldn "$profile")
      profile_mode=$(printf '%s' "$1" | cut -c 1-10) profile_owner=$3 profile_group=$4
      set -- $(ls -ldn "$source")
      [ "$(printf '%s' "$1" | cut -c 1-10) $3 $4" = "$profile_mode $profile_owner $profile_group" ] ||
        fail "Cannot preserve shell profile metadata: $profile"
      cat "$after" >"$source" || fail "Cannot stage $profile"
    else
      cat "$after" >"$source" || fail "Cannot stage $profile"
    fi
    if [ -L "$profile" ]; then
      fail "Shell profile changed during installation: $profile"
    elif [ -e "$before" ]; then
      cmp -s "$profile" "$before" || fail "Shell profile changed during installation: $profile"
    elif [ -e "$profile" ]; then
      fail "Shell profile changed during installation: $profile"
    fi
    backup=
    [ -e "$before" ] && backup=$local_stage/before
    journal "$profile" "$backup" "$source" "$(digest "$source")"
    if [ -n "$backup" ]; then
      # Set aside as a second link, then replaced in one rename, so the profile is never
      # missing.
      ln "$profile" "$backup" || fail "Cannot set aside $profile"
      cmp -s "$backup" "$before" || fail "Shell profile changed while publishing: $profile"
      ln "$source" "$local_stage/publishing" && mv -f "$local_stage/publishing" "$profile" ||
        fail "Cannot publish $profile"
    else
      ln "$source" "$profile" || fail "Cannot publish $profile"
    fi
  done <"$work/profiles"

  # A new login shell starts from the profiles, not from this installer's PATH.
  set -- env -i "HOME=$HOME" "SHELL=$SHELL"
  for variable in USER LOGNAME TERM ZDOTDIR; do
    eval "variable_set=\${$variable+x}"
    [ -z "$variable_set" ] || eval "set -- \"\$@\" \"$variable=\${$variable}\""
  done
  # Marked, so what a profile or a logout file prints around it is not taken for it.
  "$@" "$SHELL" -lic 'printf "\n@@spielorders@@%s@@\n" "$(command -v spielorders)"' \
    </dev/null >"$work/verified" 2>"$work/verified.err" &
  verifying=$!
  # A profile that waits for input or never returns must not hang the installation.
  (
    timer=
    trap 'if [ -n "$timer" ]; then kill "$timer" 2>/dev/null; wait "$timer" 2>/dev/null; fi; exit' INT TERM HUP
    sleep 30 &
    timer=$!
    wait "$timer"
    timer_status=$?
    timer=
    [ "$timer_status" -ne 0 ] || stop_verification
  ) </dev/null >/dev/null 2>&1 &
  watchdog=$!
  wait "$verifying"
  verified_status=$?
  settle_watchdog
  verifying=
  if [ "$verified_status" -eq 137 ] && ! grep -q '^@@spielorders@@' "$work/verified"; then
    fail "A fresh $shell_name did not finish within 30 seconds; the installation was rolled back. Check that its profiles finish without waiting for input."
  fi
  selected=$(sed -n 's/^@@spielorders@@\(.*\)@@$/\1/p' "$work/verified" | tail -n 1)
  if [ "$selected" != "$prefix/bin/spielorders" ]; then
    fail "A fresh $shell_name does not select the installed CLI; the installation was rolled back"
  fi
  while read -r hash name; do
    [ "$(digest "$prefix/$name")" = "$hash" ] ||
      fail "Installed content changed before verification: $prefix/$name"
  done <"$stage/files"
  "$prefix/bin/spielorders" --version </dev/null >/dev/null 2>&1 || fail "The installed CLI does not run"
  "$prefix/bin/spielorders" agent defaults --quiet </dev/null >/dev/null 2>&1 ||
    fail "The installed CLI does not find its agent guidance"
  installed=1
  printf 'Installed %s. Start a new shell to use spielorders on PATH.\n' "$prefix/bin/spielorders"
}

main "$@"

}
