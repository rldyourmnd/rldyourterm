#!/usr/bin/env bash
# Install XDG desktop entry and properly scaled icons for Wayland/X11.
# On Wayland, the compositor resolves taskbar icons from the XDG icon theme
# matched by app_id. winit's with_window_icon() is a no-op on Wayland.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LOGO="$PROJECT_ROOT/LOGO.png"
APP_NAME="rldyourterm"

ICONS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
APPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

SIZES=(16 24 32 48 64 128 256 512 1024)

if [ ! -f "$LOGO" ]; then
    echo "error: LOGO.png not found at $LOGO" >&2
    exit 1
fi

if ! python3 -c "from PIL import Image" 2>/dev/null; then
    echo "error: Python Pillow is required (pip install Pillow)" >&2
    exit 1
fi

echo "Scaling and installing icons..."
for size in "${SIZES[@]}"; do
    dir="$ICONS_DIR/${size}x${size}/apps"
    mkdir -p "$dir"
    RLD_LOGO="$LOGO" \
    RLD_ICON_OUT="$dir/$APP_NAME.png" \
    RLD_ICON_SIZE="$size" \
    python3 - <<'PY'
import os
from PIL import Image

logo = os.environ["RLD_LOGO"]
out_path = os.environ["RLD_ICON_OUT"]
size = int(os.environ["RLD_ICON_SIZE"])

img = Image.open(logo)
img = img.resize((size, size), Image.LANCZOS)
img.save(out_path)
print(f"  {size}x{size} -> {out_path}")
PY
done

echo "Installing desktop entry..."
mkdir -p "$APPS_DIR"

# Resolve binary path: prefer release build, fall back to debug
BINARY="$PROJECT_ROOT/target/release/$APP_NAME-app"
if [ ! -f "$BINARY" ]; then
    BINARY="$PROJECT_ROOT/target/debug/$APP_NAME-app"
fi

if [ ! -x "$BINARY" ]; then
    echo "error: binary not found or not executable: $BINARY" >&2
    echo "hint: run 'cargo build -p rldyourterm-app' first" >&2
    exit 1
fi

escape_desktop_exec_arg() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

EXEC_BINARY="$(escape_desktop_exec_arg "$BINARY")"
EXEC_LINE="\"$EXEC_BINARY\" --mode auto --shell fish --window-count 1"

cat > "$APPS_DIR/$APP_NAME.desktop" << EOF
[Desktop Entry]
Type=Application
Name=rldyourterm
Comment=AI-optimized terminal emulator
Exec=$EXEC_LINE
Icon=$APP_NAME
Terminal=false
Categories=System;TerminalEmulator;
StartupWMClass=$APP_NAME
StartupNotify=true
EOF

echo "Updating icon cache..."
if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t "$ICONS_DIR" 2>/dev/null || true
fi
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$APPS_DIR" 2>/dev/null || true
fi

echo "Done. Restart your compositor or log out/in for changes to take effect."
