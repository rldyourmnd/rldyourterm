#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# LAYER 1: FILE OPERATIONS INSTALLATION
# ═══════════════════════════════════════════════════════════════════════════════
# Installs: bat, fd, rg, sd, jq, yq, eza
# Platform: Linux (Ubuntu/Debian)
# Run: ./scripts/linux/install-layer-1.sh

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

arch_for_binary() {
    case "$(uname -m)" in
        x86_64 | amd64)
            echo amd64
            ;;
        aarch64 | arm64 | armv7l | armv7*)
            echo arm64
            ;;
        *)
            echo amd64
            ;;
    esac
}

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  LAYER 1: FILE OPERATIONS"
echo "  bat, fd, rg, sd, jq, yq, eza"
echo "════════════════════════════════════════════════════════════"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# PREFLIGHT CHECKS
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Running preflight checks..."

for cmd in curl apt; do
    if ! command_exists "$cmd"; then
        log_error "$cmd is required but not installed"
        exit 1
    fi
done

# Check for cargo (needed for sd)
install_rustup_if_missing

# Verify cargo is available after potential install
if ! command_exists cargo; then
    log_error "cargo still not available after rust installation"
    log_error "Please run: source \$HOME/.cargo/env && ./scripts/linux/install-layer-1.sh"
    exit 1
fi

log_success "Preflight checks passed"

# ═══════════════════════════════════════════════════════════════════════════════
# APT PACKAGES: bat, fd-find, ripgrep, jq, eza
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing apt packages..."

PACKAGES_TO_INSTALL=()
INSTALL_EZA_VIA_CARGO=false

# Check each package
if ! command_exists bat && ! command_exists batcat; then
    PACKAGES_TO_INSTALL+=("bat")
fi

if ! command_exists fd; then
    PACKAGES_TO_INSTALL+=("fd-find")
fi

if ! command_exists rg; then
    PACKAGES_TO_INSTALL+=("ripgrep")
fi

if ! command_exists jq; then
    PACKAGES_TO_INSTALL+=("jq")
fi

if ! command_exists eza; then
    if command_exists apt-cache && apt-cache show eza >/dev/null 2>&1; then
        PACKAGES_TO_INSTALL+=("eza")
    else
        INSTALL_EZA_VIA_CARGO=true
        log_warn "eza package is unavailable in apt. Will install via cargo fallback."
    fi
fi

if [ "${#PACKAGES_TO_INSTALL[@]}" -gt 0 ]; then
    sudo apt-get update
    apt_install "${PACKAGES_TO_INSTALL[@]}"
    log_success "Apt packages installed"
else
    log_info "All apt packages already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# BAT SYMLINK (Ubuntu installs as batcat)
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Setting up bat symlink..."

if command_exists batcat && ! command_exists bat; then
    mkdir -p ~/.local/bin
    ln -sf /usr/bin/batcat ~/.local/bin/bat
    log_success "bat symlink created (~/.local/bin/bat -> /usr/bin/batcat)"
elif command_exists bat; then
    log_info "bat command already available"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# FD SYMLINK (apt package is fd-find)
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Setting up fd symlink..."

if command_exists fdfind && ! command_exists fd; then
    mkdir -p ~/.local/bin
    ln -sf /usr/bin/fdfind ~/.local/bin/fd
    log_success "fd symlink created (~/.local/bin/fd -> /usr/bin/fdfind)"
elif command_exists fd; then
    log_info "fd command already available"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SD (sed replacement) - cargo
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing sd (sed replacement)..."

if ! command_exists sd; then
    cargo install --locked sd
    log_success "sd installed"
else
    log_info "sd already installed"
fi

# eza cargo fallback
if [ "$INSTALL_EZA_VIA_CARGO" = true ] && ! command_exists eza; then
    log_info "Installing eza via cargo fallback..."
    cargo install --locked eza
    log_success "eza installed via cargo"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# YQ (YAML/JSON/XML processor) - binary download
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing yq..."

if ! command_exists yq; then
    mkdir -p "$HOME/.local/bin"
    YQ_ARCH="$(arch_for_binary)"
    curl --proto '=https' --tlsv1.2 -fSsL -o "$HOME/.local/bin/yq" "https://github.com/mikefarah/yq/releases/latest/download/yq_linux_${YQ_ARCH}"
    if [ -n "${YQ_SHA256:-}" ]; then
        echo "${YQ_SHA256}  $HOME/.local/bin/yq" | sha256sum -c -
    fi
    chmod +x "$HOME/.local/bin/yq"
    log_success "yq installed"
else
    log_info "yq already installed"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# ENSURE ~/.local/bin IN PATH
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Ensuring ~/.local/bin in PATH..."

PATH_EXPORT_LINE='export PATH="$HOME/.local/bin:$PATH"'
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    touch ~/.bashrc
    if ! grep -Fqx "$PATH_EXPORT_LINE" ~/.bashrc; then
        echo "$PATH_EXPORT_LINE" >> ~/.bashrc
    fi
    export PATH="$HOME/.local/bin:$PATH"
    log_success "Added ~/.local/bin to PATH"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# VERIFICATION
# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

# Verify each tool
TOOLS="bat fd rg sd jq yq eza"
INSTALLED=0
MISSING=0

for tool in $TOOLS; do
    if command_exists "$tool"; then
        version=$($tool --version 2>/dev/null | head -1)
        echo "✅ $tool: $version"
        ((INSTALLED++))
    else
        echo "❌ $tool: NOT INSTALLED"
        ((MISSING++))
    fi
done

echo ""
echo "Installed: $INSTALLED/7"

# ═══════════════════════════════════════════════════════════════════════════════
# COMPLETE
# ═══════════════════════════════════════════════════════════════════════════════
echo ""
if [ $MISSING -eq 0 ]; then
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  LAYER 1 COMPLETE!                                         ${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Next: Run Layer 2: ./scripts/linux/install-layer-2.sh"
else
    echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
    echo -e "${YELLOW}  LAYER 1 PARTIALLY COMPLETE                                ${NC}"
    echo -e "${YELLOW}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Missing tools: $MISSING"
    echo "Try running the script again or install manually."
fi
