#!/bin/sh
# Builds CodeLibre Text and installs it with its menu entry and icon.
#   packaging/linux/install.sh                 -> ~/.local (current user)
#   sudo PREFIX=/usr/local packaging/linux/install.sh  -> every user
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
prefix=${PREFIX:-$HOME/.local}

cargo build --release --manifest-path "$root/Cargo.toml"

install -Dm755 "$root/target/release/codelibre_text" "$prefix/bin/codelibre_text"
install -Dm644 "$root/assets/icons/codelibre_text.png" "$prefix/share/icons/hicolor/256x256/apps/codelibre_text.png"
install -Dm644 "$root/packaging/linux/codelibre_text.desktop" "$prefix/share/applications/codelibre_text.desktop"

# Refresh the menu and icon caches when the tools are available.
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" || true

echo "Installed in $prefix (make sure $prefix/bin is in your PATH)."
