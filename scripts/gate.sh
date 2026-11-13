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

  # Replace generated bundles, rather than overlaying obsolete embedded resources.
  rm -rf target/native/Build/Products/Debug/SpielOrders.app
  rm -rf target/native-validation/Build/Products/Release/SpielOrders.app

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

  test ! -e "$application/Contents/Helpers"
  cargo build -q -p spielorders-cli --release --locked
  cli=target/release/spielorders
  codesign --verify --strict "$cli"
  cli_entitlements=target/native-validation/cli-entitlements.plist
  codesign --display --entitlements - --xml "$cli" > "$cli_entitlements"
  if [[ -s "$cli_entitlements" ]]; then
    for key in app-sandbox inherit; do
      if [[ "$(/usr/libexec/PlistBuddy -c "Print :com.apple.security.$key" "$cli_entitlements" 2>/dev/null || true)" == true ]]; then
        echo "standalone CLI must run with ordinary user permissions" >&2
        exit 1
      fi
    done
  fi

  if [[ "${SPIELORDERS_EXCLUSIVE_UI:-0}" == 1 ]]; then
    "${native[@]}" -derivedDataPath target/native -configuration Debug test-without-building
  fi
fi

echo "gate passed"
