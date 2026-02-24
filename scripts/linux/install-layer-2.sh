#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# LAYER 2: PRODUCTIVITY INSTALLATION
# ═══════════════════════════════════════════════════════════════════════════════
# Installs: fzf, zoxide, atuin, uv, bun, watchexec, glow, bottom, hyperfine
# Platform: Linux (Ubuntu/Debian)
# Run: ./scripts/linux/install-layer-2.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Logging
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check if command exists
command_exists() { command -v "$1" &>/dev/null; }

apt_install() {
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$@"
}

install_rustup_if_missing() {
    if command_exists cargo; then
        return 0
    fi

    log_warn "cargo not found. Installing rust..."
    RUSTUP_INSTALLER="$(mktemp)"
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$RUSTUP_INSTALLER"
    sh "$RUSTUP_INSTALLER" -y
    rm -f "$RUSTUP_INSTALLER"

    if [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    fi
}

resolve_release_binary_url() {
    local repo="$1"
    local pattern="$2"
    local release_json

    release_json="$(curl -fsSL "https://api.github.com/repos/${repo}/releases/latest" 2>/dev/null || true)"
    if [ -z "$release_json" ]; then
        return 1
    fi

    echo "$release_json" | grep -oP '"browser_download_url": "\K([^\"]+)' | grep -E "$pattern" | head -n 1
}

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  LAYER 2: PRODUCTIVITY"
echo "  fzf, zoxide, atuin, uv, bun, watchexec, glow, bottom, hyperfine"
echo "════════════════════════════════════════════════════════════"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# PREFLIGHT CHECKS
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Running preflight checks..."

for cmd in curl git tar; do
    if ! command_exists "$cmd"; then
        log_error "$cmd is required but not installed"
        exit 1
    fi
done

# Check for cargo (needed for some tools)
install_rustup_if_missing

if ! command_exists cargo; then
    log_error "cargo still not available. Please install manually and re-run."
    exit 1
fi

log_success "Preflight checks passed"

# ═══════════════════════════════════════════════════════════════════════════════
# FZF - Fuzzy finder
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing fzf..."

if ! command_exists fzf; then
    if [ -d "$HOME/.fzf/.git" ]; then
        git -C "$HOME/.fzf" pull --ff-only
    else
        rm -rf "$HOME/.fzf"
        git clone --depth 1 https://github.com/junegunn/fzf.git "$HOME/.fzf"
    fi
    ~/.fzf/install --key-bindings --completion --no-update-rc --no-bash --no-zsh
    log_success "fzf installed"
else
    log_info "fzf already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# ZOXIDE - Smart cd with frecency
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing zoxide..."

if ! command_exists zoxide; then
    cargo install --locked zoxide
    log_success "zoxide installed"
else
    log_info "zoxide already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# ATUIN - SQLite history with sync
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing atuin..."

if ! command_exists atuin; then
    ATUIN_INSTALLER="$(mktemp)"
    curl --proto '=https' --tlsv1.2 -fsSL https://setup.atuin.sh -o "$ATUIN_INSTALLER"
    sh "$ATUIN_INSTALLER"
    rm -f "$ATUIN_INSTALLER"
    log_success "atuin installed"
else
    log_info "atuin already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# UV - Fast Python package manager
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing uv..."

if ! command_exists uv; then
    UV_INSTALLER="$(mktemp)"
    curl --proto '=https' --tlsv1.2 -fsSL https://astral.sh/uv/install.sh -o "$UV_INSTALLER"
    sh "$UV_INSTALLER"
    rm -f "$UV_INSTALLER"
    # Add to PATH for current session
    if [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    fi
    log_success "uv installed"
else
    log_info "uv already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# BUN - Fast JavaScript runtime
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing bun..."

if ! command_exists bun; then
    BUN_INSTALLER="$(mktemp)"
    curl --proto '=https' --tlsv1.2 -fsSL https://bun.sh/install -o "$BUN_INSTALLER"
    bash "$BUN_INSTALLER"
    rm -f "$BUN_INSTALLER"
    # Add to PATH for current session
    export BUN_INSTALL="$HOME/.bun"
    export PATH="$BUN_INSTALL/bin:$PATH"
    log_success "bun installed"
else
    log_info "bun already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# WATCHEXEC - File watcher
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing watchexec..."

if ! command_exists watchexec; then
    cargo install --locked watchexec-cli
    log_success "watchexec installed"
else
    log_info "watchexec already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# GLOW - Markdown renderer
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing glow..."

if ! command_exists glow; then
    if command_exists apt-cache; then
        if apt-cache show glow >/dev/null 2>&1; then
            log_info "Installing glow from apt..."
            apt_install glow
            log_success "glow installed"
        else
            log_warn "glow package is not available in apt repositories"

            GLOW_VERSION="$(curl -fsSL https://api.github.com/repos/charmbracelet/glow/releases/latest \
              | grep -oP '\"tag_name\": \"\\K(.*)(?=\")' || echo "v2.1.1")"
            GLOW_TMP_DIR="$(mktemp -d)"
            GLOW_ARCHIVE="$GLOW_TMP_DIR/glow.tar.gz"
            GLOW_ASSET_URL="$(resolve_release_binary_url "charmbracelet/glow" "glow.*${GLOW_VERSION#v}.*(linux|Linux).*(tar\\.gz)$")"
            GLOW_CHECKSUMS_URL="$(resolve_release_binary_url "charmbracelet/glow" "checksums\\.txt$")"

            if [ -z "$GLOW_ASSET_URL" ]; then
                log_error "Could not locate a suitable glow binary archive for ${GLOW_VERSION}"
                rm -rf "$GLOW_TMP_DIR"
                exit 1
            fi

            curl --proto '=https' --tlsv1.2 -fSsL "$GLOW_ASSET_URL" -o "$GLOW_ARCHIVE"
            if [ -n "$GLOW_CHECKSUMS_URL" ]; then
                curl --proto '=https' --tlsv1.2 -fSsL "$GLOW_CHECKSUMS_URL" -o "$GLOW_TMP_DIR/checksums.txt"
                expected_sum="$(grep " $(basename "$GLOW_ASSET_URL")\$" "$GLOW_TMP_DIR/checksums.txt" | awk '{print $1}' | head -n 1 || true)"
                if [ -n "$expected_sum" ]; then
                    echo "${expected_sum}  ${GLOW_ARCHIVE}" | sha256sum -c -
                else
                    log_warn "Could not resolve checksum for $(basename "$GLOW_ASSET_URL"), continuing without checksum validation"
                fi
            else
                log_warn "Glow checksums asset not found, continuing without checksum validation"
            fi
            tar -xzf "$GLOW_ARCHIVE" -C "$GLOW_TMP_DIR"

            mkdir -p "$HOME/.local/bin"
            if [ -x "$GLOW_TMP_DIR/glow" ]; then
                mv "$GLOW_TMP_DIR/glow" "$HOME/.local/bin/glow"
            elif [ -x "$GLOW_TMP_DIR/bin/glow" ]; then
                mv "$GLOW_TMP_DIR/bin/glow" "$HOME/.local/bin/glow"
            else
                rm -rf "$GLOW_TMP_DIR"
                log_error "Glow archive did not contain a glow binary"
                exit 1
            fi

            chmod +x "$HOME/.local/bin/glow"
            log_success "glow installed"
            rm -rf "$GLOW_TMP_DIR"
        fi
    else
        log_error "apt-cache not found; cannot install glow reliably."
        exit 1
    fi
else
    log_info "glow already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# BOTTOM - System monitor
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing bottom..."

if ! command_exists btm; then
    cargo install --locked bottom
    log_success "bottom installed"
else
    log_info "bottom already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# HYPERFINE - Benchmarking tool
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing hyperfine..."

if ! command_exists hyperfine; then
    cargo install --locked hyperfine
    log_success "hyperfine installed"
else
    log_info "hyperfine already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# FISH CONFIG - No changes needed (conditional sourcing)
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Fish config uses conditional sourcing - no changes needed"
log_info "Tools will be auto-detected on next shell startup"

PATH_EXPORT_LINE='export PATH="$HOME/.local/bin:$PATH"'
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    touch ~/.bashrc
    if ! grep -Fqx "$PATH_EXPORT_LINE" ~/.bashrc; then
        echo "$PATH_EXPORT_LINE" >> ~/.bashrc
    fi
    export PATH="$HOME/.local/bin:$PATH"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# VERIFICATION
# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

TOOLS="fzf zoxide atuin uv bun watchexec glow btm hyperfine"
INSTALLED=0
MISSING=0

for tool in $TOOLS; do
    if command_exists "$tool"; then
        version=$($tool --version 2>/dev/null | head -1 || echo "installed")
        echo "✅ $tool: $version"
        ((INSTALLED++))
    else
        echo "❌ $tool: NOT INSTALLED"
        ((MISSING++))
    fi
done

echo ""
echo "Installed: $INSTALLED/9"

# ═══════════════════════════════════════════════════════════════════════════════
# COMPLETE
# ═══════════════════════════════════════════════════════════════════════════════
echo ""
if [ $MISSING -eq 0 ]; then
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  LAYER 2 COMPLETE!                                         ${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Next steps:"
    echo "1. Restart terminal or run: exec fish"
    echo "2. Run Layer 3: ./scripts/linux/install-layer-3.sh"
else
    echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
    echo -e "${YELLOW}  LAYER 2 PARTIALLY COMPLETE                                ${NC}"
    echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Missing tools: $MISSING"
    echo "Try running the script again or install manually."
fi
