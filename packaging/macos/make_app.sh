#!/bin/sh
# Builds "CodeLibre Text.app" (with its Dock / Finder icon) in target/release.
# Drag it to /Applications to install it.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
app="$root/target/release/CodeLibre Text.app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)

cargo build --release --manifest-path "$root/Cargo.toml"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/target/release/codelibre_text" "$app/Contents/MacOS/codelibre_text"
cp "$root/assets/icons/codelibre_text.icns" "$app/Contents/Resources/codelibre_text.icns"
sed "s/@VERSION@/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"

echo "Created $app"
