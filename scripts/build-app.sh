#!/usr/bin/env bash
#
# Build the distributable macOS app: the Rust helper with libusb linked
# statically, bundled inside the Flutter .app.
#
# The helper must be the `vendored` build. An ordinary `cargo build` links the
# Homebrew libusb dylib, which is absent on most machines the app is copied to
# and makes every command fail at launch with a dyld error.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app="$root/gui/build/macos/Build/Products/Release/aw_flasher.app"

echo "==> aw-tool (vendored libusb)"
cargo build --manifest-path "$root/Cargo.toml" --release --features vendored

if otool -L "$root/target/release/aw-tool" | grep -q homebrew; then
    echo "aw-tool still links a Homebrew dylib — the vendored build did not take" >&2
    exit 1
fi

echo "==> Flutter app"
(cd "$root/gui" && flutter build macos --release)

echo "==> bundling helper"
cp "$root/target/release/aw-tool" "$app/Contents/Resources/aw-tool"

# Adding a file invalidates the signature Flutter produced, and macOS refuses
# to launch a bundle whose signature no longer matches. Ad-hoc re-signing is
# enough for an internal tool; a distributed build would sign with a Developer
# ID here instead.
echo "==> re-signing"
codesign --force --sign - \
    --entitlements "$root/gui/macos/Runner/Release.entitlements" \
    "$app"

codesign --verify --strict "$app"

echo
echo "완료: $app"
