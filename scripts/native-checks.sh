#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

cargo build -p spielorders --locked
swift test --package-path apps/macos -Xlinker "$PWD/target/debug/libspielorders.a"

native=(xcodebuild -project apps/macos/SpielOrders.xcodeproj -scheme SpielOrders
  -destination 'platform=macOS,arch=arm64' -quiet)

# Decision source: Desktop acceptance.
"${native[@]}" -derivedDataPath target/native -configuration Debug build-for-testing
"${native[@]}" -derivedDataPath target/native-validation -configuration Release build \
  SPIELORDERS_APP_IDENTIFIER=com.spielsoft.SpielOrders.Validation

application=target/native-validation/Build/Products/Release/SpielOrders.app
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$application/Contents/Info.plist")" = com.spielsoft.SpielOrders.Validation
test -n "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$application/Contents/Info.plist")"
test -n "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$application/Contents/Info.plist")"
swift scripts/check-native-icon.swift "$application" icons/SpielOrders.iconset/icon_512x512@2x.png
codesign --verify --strict "$application"
codesign --display --entitlements - --xml "$application" > target/native-validation/release-entitlements.plist
test "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' target/native-validation/release-entitlements.plist)" = true
if /usr/libexec/PlistBuddy -c 'Print :com.apple.security.temporary-exception.files.absolute-path.read-only' target/native-validation/release-entitlements.plist >/dev/null 2>&1; then
  echo "release app carries a testing-only filesystem exception" >&2
  exit 1
fi

if [[ "${SPIELORDERS_EXCLUSIVE_UI:-0}" == 1 ]]; then
  "${native[@]}" -derivedDataPath target/native -configuration Debug test-without-building
else
  echo "native build and headless checks passed; interactive acceptance is not run"
fi
