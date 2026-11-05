#!/bin/sh
# Installs syzygy's desktop file and icons for the current user, so launchers
# list syzygy and the compositor matches its window to the icon. The binary
# itself must be on PATH as `syzygy` (e.g. `cargo install --path crates/syzygy`).
set -eu

assets="$(dirname "$0")/../crates/syzygy/assets"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
id=com.lorenzorapetti.syzygy

install -Dm644 "$assets/$id.desktop" -t "$data/applications"
install -Dm644 "$assets/$id.svg" -t "$data/icons/hicolor/scalable/apps"
for dir in "$assets"/icons/*; do
    install -Dm644 "$dir/$id.png" -t "$data/icons/hicolor/$(basename "$dir")/apps"
done

# Refreshing is best effort: the files are installed either way.
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$data/applications" || true
fi
# Only refresh a cache that exists: creating one would hide icons that other
# apps install here later without refreshing it.
if [ -f "$data/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -f -t "$data/icons/hicolor" || true
fi
echo "Installed to $data"
