#!/usr/bin/env bash
# The milestone gate: scripts/check.sh, then UI-test compilation, the signed Validation
# build, and its bundle and entitlement checks. SPIELORDERS_EXCLUSIVE_UI=1 also runs the
# UI tests, which take the foreground.
set -euo pipefail
cd "$(dirname "$0")/.."

bash scripts/check.sh

if [[ "$(uname -s)" == Darwin ]]; then
  native=(xcodebuild -project apps/macos/SpielOrders.xcodeproj -scheme SpielOrders
    -destination 'platform=macOS,arch=arm64' -quiet)

  # Decision source: Desktop acceptance.
  "${native[@]}" -derivedDataPath target/native -configuration Debug build-for-testing
  "${native[@]}" -derivedDataPath target/native-validation -configuration Release build \
    SPIELORDERS_APP_IDENTIFIER=com.spielsoft.SpielOrders.Validation

  application=target/native-validation/Build/Products/Release/SpielOrders.app
  entitlements=target/native-validation/release-entitlements.plist
  test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$application/Contents/Info.plist")" = com.spielsoft.SpielOrders.Validation
  test -n "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$application/Contents/Info.plist")"
  test -n "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$application/Contents/Info.plist")"
  swift scripts/check-native-icon.swift "$application" icons/SpielOrders.iconset/icon_512x512@2x.png
  codesign --verify --strict --deep "$application"
  codesign --display --entitlements - --xml "$application" > "$entitlements"
  test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$entitlements")" = true
  if /usr/libexec/PlistBuddy -c 'Print :com.apple.security.temporary-exception.files.absolute-path.read-only' "$entitlements" >/dev/null 2>&1; then
    echo "release app carries a testing-only filesystem exception" >&2
    exit 1
  fi

  cli_bundle="$application/Contents/Helpers/SpielOrdersCLI.app"
  cli="$cli_bundle/Contents/MacOS/spielorders"
  test -L "$cli"
  test "$(readlink "$cli")" = "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$cli_bundle/Contents/Info.plist")"
  backend="$cli_bundle/Contents/Helpers/spielorders-backend"
  test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$cli_bundle/Contents/Info.plist")" = com.spielsoft.SpielOrders.Validation.CLI
  codesign --verify --strict "$cli_bundle"
  codesign --verify --strict "$backend"
  cli_entitlements=target/native-validation/cli-entitlements.plist
  backend_entitlements=target/native-validation/cli-backend-entitlements.plist
  codesign --display --entitlements - --xml "$cli_bundle" > "$cli_entitlements"
  codesign --display --entitlements - --xml "$backend" > "$backend_entitlements"
  test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$cli_entitlements")" = true
  test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.files.user-selected.read-write' "$cli_entitlements")" = true
  test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.files.bookmarks.app-scope' "$cli_entitlements")" = true
  if /usr/libexec/PlistBuddy -c 'Print :com.apple.security.inherit' "$cli_entitlements" >/dev/null 2>&1; then
    echo "external CLI launcher must own its sandbox" >&2
    exit 1
  fi
  test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$backend_entitlements")" = true
  test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.inherit' "$backend_entitlements")" = true
  diff -qr crates/spielorders/agent-guidance "$cli_bundle/Contents/Resources/agent-guidance"
  "$cli" --version
  "$cli" agent defaults --quiet
  # Agents may reach the CLI through a link placed on PATH, outside its bundle.
  path_link=target/native-validation/path-link
  rm -rf "$path_link"
  mkdir -p "$path_link"
  ln -s "$PWD/$cli" "$path_link/spielorders"
  "$path_link/spielorders" agent defaults --quiet

  if [[ "${SPIELORDERS_EXCLUSIVE_UI:-0}" == 1 ]]; then
    "${native[@]}" -derivedDataPath target/native -configuration Debug test-without-building
  fi
fi

echo "gate passed"
