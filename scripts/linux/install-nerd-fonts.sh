#!/usr/bin/env bash
set -euo pipefail

log() {
    printf '[nerd-fonts] %s\n' "$1"
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

require_cmd curl
require_cmd unzip
require_cmd fc-cache

FONT_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/fonts"
JETBRAINS_DIR="$FONT_ROOT/JetBrainsMonoNerd"
SYMBOLS_DIR="$FONT_ROOT/NerdFontsSymbolsOnly"
TMP_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$JETBRAINS_DIR" "$SYMBOLS_DIR"

log "Downloading JetBrainsMono Nerd Font..."
curl -fsSL \
    "https://github.com/ryanoasis/nerd-fonts/releases/latest/download/JetBrainsMono.zip" \
    -o "$TMP_DIR/JetBrainsMono.zip"

log "Extracting JetBrainsMono Nerd Font..."
unzip -oq "$TMP_DIR/JetBrainsMono.zip" "*.ttf" -d "$JETBRAINS_DIR"

log "Downloading NerdFonts Symbols Only..."
curl -fsSL \
    "https://github.com/ryanoasis/nerd-fonts/releases/latest/download/NerdFontsSymbolsOnly.zip" \
    -o "$TMP_DIR/NerdFontsSymbolsOnly.zip"

log "Extracting NerdFonts Symbols Only..."
unzip -oq "$TMP_DIR/NerdFontsSymbolsOnly.zip" "*.ttf" -d "$SYMBOLS_DIR"

log "Rebuilding font cache..."
fc-cache -f "$FONT_ROOT"

log "Installed fonts:"
find "$JETBRAINS_DIR" "$SYMBOLS_DIR" -maxdepth 1 -type f -name "*.ttf" | sort

log "Done. Restart rldyourterm to pick up new fonts."
