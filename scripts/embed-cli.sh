#!/bin/sh
# The native launcher owns its sandbox and saved grants. The unchanged portable
# executable inherits that process at exec; it has no permission-UI protocol.
set -eu
cd "$SRCROOT/../.."
export PATH="$HOME/.cargo/bin:$PATH"
if [ "$CONFIGURATION" = Release ]; then
    cargo build -p spielorders-cli --release --locked
    cli_profile=release
else
    cargo build -p spielorders-cli --locked
    cli_profile=debug
fi
cli_bundle="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/Helpers/SpielOrdersCLI.app"
mkdir -p "$cli_bundle/Contents/MacOS" "$cli_bundle/Contents/Helpers" "$cli_bundle/Contents/Resources"
cp apps/macos/CLI/Info.plist "$cli_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $PRODUCT_BUNDLE_IDENTIFIER.CLI" "$cli_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $MARKETING_VERSION" "$cli_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $CURRENT_PROJECT_VERSION" "$cli_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :LSMinimumSystemVersion $MACOSX_DEPLOYMENT_TARGET" "$cli_bundle/Contents/Info.plist"
cp "target/$cli_profile/spielorders" "$cli_bundle/Contents/Helpers/spielorders-backend"
ditto crates/spielorders/agent-guidance "$cli_bundle/Contents/Resources/agent-guidance"
xcrun swiftc -parse-as-library -swift-version 6 -O \
    -target "arm64-apple-macosx$MACOSX_DEPLOYMENT_TARGET" \
    apps/macos/CLI/Launcher.swift apps/macos/App/AgentAuthorizationContract.swift \
    -o "$cli_bundle/Contents/MacOS/SpielOrdersCLIAuthorization"
ln -sf SpielOrdersCLIAuthorization "$cli_bundle/Contents/MacOS/spielorders"
cli_signer="${EXPANDED_CODE_SIGN_IDENTITY:--}"
codesign --force --sign "$cli_signer" --options runtime \
    --identifier "$PRODUCT_BUNDLE_IDENTIFIER.CLI.Backend" \
    --entitlements apps/macos/CLI/Backend.entitlements \
    "$cli_bundle/Contents/Helpers/spielorders-backend"
codesign --force --sign "$cli_signer" --options runtime \
    --entitlements apps/macos/App/SpielOrders.entitlements "$cli_bundle"
