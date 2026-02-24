#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  starship-profile list
  starship-profile current
  starship-profile <stable-max|ultra-max>

Behavior:
  - Reads profiles from ~/.config/starship/profiles
  - Falls back to repository profiles when run from repo scripts directory
  - Validates profile with starship before applying
  - Applies by copying profile to ~/.config/starship.toml
EOF
}

log() {
    printf '[starship-profile] %s\n' "$1"
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG_ROOT="${XDG_CONFIG_HOME:-$HOME/.config}"
STARSHIP_DIR="$CONFIG_ROOT/starship"
PROFILE_DIR="$STARSHIP_DIR/profiles"
TARGET_CONFIG="$CONFIG_ROOT/starship.toml"
MARKER_FILE="$STARSHIP_DIR/.active-profile"

FALLBACK_PROFILE_DIR="$REPO_ROOT/configs/starship/profiles"

mkdir -p "$STARSHIP_DIR"

available_profiles() {
    if [ -d "$PROFILE_DIR" ] && ls "$PROFILE_DIR"/*.toml >/dev/null 2>&1; then
        find "$PROFILE_DIR" -maxdepth 1 -type f -name '*.toml' -printf '%f\n' \
            | sed 's/\.toml$//' | sort
        return
    fi
    if [ -d "$FALLBACK_PROFILE_DIR" ] && ls "$FALLBACK_PROFILE_DIR"/*.toml >/dev/null 2>&1; then
        find "$FALLBACK_PROFILE_DIR" -maxdepth 1 -type f -name '*.toml' -printf '%f\n' \
            | sed 's/\.toml$//' | sort
        return
    fi
}

resolve_profile_path() {
    local profile="$1"
    if [ -f "$PROFILE_DIR/$profile.toml" ]; then
        printf '%s\n' "$PROFILE_DIR/$profile.toml"
        return
    fi
    if [ -f "$FALLBACK_PROFILE_DIR/$profile.toml" ]; then
        printf '%s\n' "$FALLBACK_PROFILE_DIR/$profile.toml"
        return
    fi
    return 1
}

if [ "${1:-}" = "" ]; then
    usage
    exit 1
fi

case "$1" in
    -h|--help|help)
        usage
        exit 0
        ;;
    list)
        available_profiles
        exit 0
        ;;
    current)
        if [ -f "$MARKER_FILE" ]; then
            cat "$MARKER_FILE"
        else
            echo "unknown"
        fi
        exit 0
        ;;
esac

PROFILE="$1"
SOURCE_PROFILE="$(resolve_profile_path "$PROFILE" || true)"

if [ -z "$SOURCE_PROFILE" ]; then
    log "Profile '$PROFILE' not found."
    echo "Available profiles:"
    available_profiles || true
    exit 1
fi

if command -v starship >/dev/null 2>&1; then
    STARSHIP_CONFIG="$SOURCE_PROFILE" starship module character >/dev/null
fi

if [ -f "$TARGET_CONFIG" ]; then
    cp "$TARGET_CONFIG" "$TARGET_CONFIG.bak.$(date +%Y%m%d_%H%M%S)"
fi

cp "$SOURCE_PROFILE" "$TARGET_CONFIG"
echo "$PROFILE" > "$MARKER_FILE"

log "Applied profile: $PROFILE"
log "Config: $TARGET_CONFIG"
log "Tip: run 'exec fish' if prompt does not refresh immediately."
