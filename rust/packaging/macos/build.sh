#!/bin/sh
# Builds Mindloom.app (universal: Apple silicon + Intel) and Mindloom-<version>-mac-universal.dmg into rust/dist/.
# Usage: rust/packaging/macos/build.sh   (needs Xcode command line tools and both Rust targets)
set -eu
cd "$(dirname "$0")/../.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release -p mindloom --target aarch64-apple-darwin
cargo build --release -p mindloom --target x86_64-apple-darwin

OUT=dist
APP="$OUT/Mindloom.app"
rm -rf "$APP" "$OUT/dmg"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/mindloom" \
  target/aarch64-apple-darwin/release/mindloom target/x86_64-apple-darwin/release/mindloom
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# icon: every size macOS asks for, from the 1024 px master
ICONSET="$OUT/Mindloom.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s app/assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) app/assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Mindloom.icns"
rm -rf "$ICONSET"

# ad-hoc signature (Apple silicon refuses to run unsigned code)
codesign --force --deep --sign - "$APP"

mkdir -p "$OUT/dmg"
cp -R "$APP" "$OUT/dmg/"
ln -s /Applications "$OUT/dmg/Applications"
DMG="$OUT/Mindloom-$VERSION-mac-universal.dmg"
rm -f "$DMG"
hdiutil create -volname "Mindloom $VERSION" -srcfolder "$OUT/dmg" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$OUT/dmg"
echo "$DMG"
