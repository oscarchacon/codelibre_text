#!/bin/sh
# Builds "CodeLibre Text.app" (with its Dock / Finder icon) in target/release, or in
# target/$CARGO_BUILD_TARGET/release when that variable chooses another architecture.
# Drag it to /Applications to install it.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
release="$root/target/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release"
app="$release/CodeLibre Text.app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)

cargo build --release --manifest-path "$root/Cargo.toml"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$release/codelibre_text" "$app/Contents/MacOS/codelibre_text"
cp "$root/assets/icons/codelibre_text.icns" "$app/Contents/Resources/codelibre_text.icns"
# Bundle versions are numbers only: 0.1.0-alpha is 0.1.0 here.
sed "s/@VERSION@/${version%%-*}/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"

echo "Created $app"
