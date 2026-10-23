#!/usr/bin/env bash
# Build Kosha and install it for this user (no root): ~/.local/bin/kosha-app (the window),
# ~/.local/bin/kosha (command line), a launcher entry and the icon.
#   bash tools/install-linux.sh            build in release mode and install
#   bash tools/install-linux.sh --remove   take it out again (your settings and indexes stay)
set -euo pipefail
cd "$(dirname "$0")/.."
BIN="$HOME/.local/bin"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
DESKTOP="$APPS/io.github.shatyrko.kosha.desktop"

if [[ ${1:-} == --remove ]]; then
    rm -f "$BIN/kosha-app" "$BIN/kosha" "$DESKTOP" "$ICONS/scalable/apps/io.github.shatyrko.kosha.svg" \
          "$ICONS/128x128/apps/io.github.shatyrko.kosha.png"
    echo "Kosha removed (settings: ~/.config/kosha, indexes: ~/.cache/kosha)."
    exit 0
fi

cargo build --release -p kosha-app -p kosha-cli
install -Dm755 target/release/kosha-app "$BIN/kosha-app"
install -Dm755 target/release/kosha "$BIN/kosha"
install -Dm644 src-tauri/icons/icon.svg "$ICONS/scalable/apps/io.github.shatyrko.kosha.svg"
install -Dm644 src-tauri/icons/128x128.png "$ICONS/128x128/apps/io.github.shatyrko.kosha.png"
mkdir -p "$APPS"
cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=Kosha
GenericName=Dictionary
Comment=Look words up in your dictionaries
Exec=$BIN/kosha-app %U
Icon=io.github.shatyrko.kosha
Terminal=false
Categories=Office;Dictionary;Education;
Keywords=dictionary;translate;lingvo;dsl;words;
StartupWMClass=kosha-app
EOF
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
echo "Installed: $BIN/kosha-app, $BIN/kosha, launcher entry \"Kosha\"."
