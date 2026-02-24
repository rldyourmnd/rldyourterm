#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# LAYER 3: GITHUB & GIT INSTALLATION
# ═══════════════════════════════════════════════════════════════════════════════
# Installs: gh CLI, lazygit, delta
# Platform: Linux (Ubuntu/Debian)
# Run: ./scripts/linux/install-layer-3.sh
#
# Verified (February 2026):
# - gh CLI: Score 83.2 (/websites/cli_github)
# - lazygit: Score 46 (/jesseduffield/lazygit)
# - delta: Popular (dandavison/delta)

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
        x86_64|amd64)
            echo x86_64
            ;;
        aarch64|arm64|armv8*|armv7*)
            echo arm64
            ;;
        *)
            echo x86_64
            ;;
    esac
}

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  LAYER 3: GITHUB & GIT"
echo "  gh CLI, lazygit, delta"
echo "════════════════════════════════════════════════════════════"
echo ""

# ═══════════════════════════════════════════════════════════════════════════════
# GH CLI - GitHub in terminal
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing gh CLI..."

if ! command_exists gh; then
    if sudo -n true 2>/dev/null; then
        sudo apt-get update
        apt_install gh
        log_success "gh CLI installed"
    else
        log_warn "gh CLI requires sudo. Run manually:"
        echo "  sudo apt update && sudo apt install -y gh"
    fi
else
    log_info "gh CLI already installed: $(gh --version 2>/dev/null | head -1)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# LAZYGIT - Terminal UI for git (download latest binary)
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing lazygit..."

if ! command_exists lazygit || lazygit --version 2>/dev/null | grep -q "unversioned"; then
    # Get latest version from GitHub API
    LAZYGIT_VERSION=$(curl -s https://api.github.com/repos/jesseduffield/lazygit/releases/latest | grep -oP '"tag_name": "\K[^"]+' || echo "v0.59.0")

    log_info "Downloading lazygit ${LAZYGIT_VERSION}..."

    TEMP_DIR="$(mktemp -d)"

    # Download and extract
    LAZYGIT_ARCH="$(arch_for_binary)"
    LAZYGIT_URL="https://github.com/jesseduffield/lazygit/releases/download/${LAZYGIT_VERSION}/lazygit_${LAZYGIT_VERSION#v}_Linux_${LAZYGIT_ARCH}.tar.gz"
    LAZYGIT_CHECKSUMS_URL="https://github.com/jesseduffield/lazygit/releases/download/${LAZYGIT_VERSION}/checksums.txt"

    if curl --proto '=https' --tlsv1.2 -fSsL "$LAZYGIT_URL" -o "$TEMP_DIR/lazygit.tar.gz"; then
        if curl --proto '=https' --tlsv1.2 -fSsL "$LAZYGIT_CHECKSUMS_URL" -o "$TEMP_DIR/checksums.txt"; then
            expected_sum="$(grep " $(basename "$LAZYGIT_URL")\$" "$TEMP_DIR/checksums.txt" | awk '{print $1}' | head -n 1 || true)"
            if [ -n "$expected_sum" ]; then
                echo "${expected_sum}  $TEMP_DIR/lazygit.tar.gz" | sha256sum -c -
            else
                log_warn "Could not resolve lazygit checksum from checksums.txt"
            fi
        else
            log_warn "lazygit checksums.txt not found, continuing without checksum validation"
        fi

        tar -xzf "$TEMP_DIR/lazygit.tar.gz" -C "$TEMP_DIR"
        # Install to ~/.local/bin (no sudo needed)
        mkdir -p ~/.local/bin
        mv "$TEMP_DIR/lazygit" ~/.local/bin/lazygit
        chmod +x ~/.local/bin/lazygit

        log_success "lazygit ${LAZYGIT_VERSION} installed to ~/.local/bin/"
    else
        log_error "Failed to download lazygit"
    fi

    # Cleanup
    rm -rf "$TEMP_DIR"
else
    log_info "lazygit already installed: $(lazygit --version 2>/dev/null | head -1)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# DELTA - Git diff viewer with syntax highlighting
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing delta..."

if ! command_exists delta; then
    install_rustup_if_missing
    cargo install --locked git-delta
    log_success "delta installed"
else
    log_info "delta already installed: $(delta --version 2>/dev/null | head -1)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# ENSURE ~/.local/bin IN PATH
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Ensuring ~/.local/bin in PATH..."

if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    PATH_EXPORT_LINE='export PATH="$HOME/.local/bin:$PATH"'
    touch ~/.bashrc
    if ! grep -Fqx "$PATH_EXPORT_LINE" ~/.bashrc; then
        echo "$PATH_EXPORT_LINE" >> ~/.bashrc
    fi
    export PATH="$HOME/.local/bin:$PATH"
    log_success "Added ~/.local/bin to PATH"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# CATPPUCCIN THEME FOR BAT (required for delta)
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Installing Catppuccin Mocha theme for bat..."

BAT_CONFIG_DIR="$(bat --config-dir 2>/dev/null || true)"
BAT_THEMES_DIR="${BAT_CONFIG_DIR}/themes"
if [ -n "$BAT_CONFIG_DIR" ]; then
    mkdir -p "$BAT_THEMES_DIR"
    if [ ! -f "$BAT_THEMES_DIR/Catppuccin Mocha.tmTheme" ]; then
        curl --proto '=https' --tlsv1.2 -fSsL "https://github.com/catppuccin/bat/raw/main/themes/Catppuccin%20Mocha.tmTheme" -o "$BAT_THEMES_DIR/Catppuccin Mocha.tmTheme"
        bat cache --build 2>/dev/null
        log_success "Catppuccin Mocha theme installed for bat"
    else
        log_info "Catppuccin Mocha theme already installed"
    fi
else
    log_warn "bat not available; skipping bat theme installation"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# DELTA CONFIGURATION
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Configuring delta..."

if command_exists delta; then
    # Set delta as git pager
    git config --global core.pager "delta"

    # Delta options
    git config --global delta.line-numbers true
    git config --global delta.side-by-side true
    git config --global delta.navigate true
    git config --global delta.dark true
    git config --global delta.syntax-theme "Catppuccin Mocha" 2>/dev/null || git config --global delta.syntax-theme "Monokai Extended"
    git config --global delta.hyperlinks true
    git config --global delta.hyperlinks-commit-link-format "https://github.com/{host}/{org}/{repo}/commit/{commit}"

    # Interactive diff filter
    git config --global interactive.diffFilter "delta --color-only"

    # Merge conflict style
    git config --global merge.conflictstyle diff3
    git config --global diff.colorMoved default

    log_success "delta configured"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# GIT ALIASES
# ═══════════════════════════════════════════════════════════════════════════════
log_info "Configuring git aliases..."

git config --global alias.co checkout
git config --global alias.br branch
git config --global alias.ci commit
git config --global alias.st status
git config --global alias.unstage 'reset HEAD --'
git config --global alias.last 'log -1 HEAD'
git config --global alias.visual '!lazygit'
git config --global alias.lg 'log --oneline --graph --all'

log_success "git aliases configured"

# ═══════════════════════════════════════════════════════════════════════════════
# VERIFICATION
# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

# Verify gh CLI
if command_exists gh; then
    echo "✅ gh CLI: $(gh --version 2>/dev/null | head -1)"
else
    echo "❌ gh CLI: NOT INSTALLED (run: sudo apt install -y gh)"
fi

# Verify lazygit
if command_exists lazygit; then
    echo "✅ lazygit: $(lazygit --version 2>/dev/null | head -1 || echo 'installed')"
else
    echo "❌ lazygit: NOT INSTALLED"
fi

# Verify delta
if command_exists delta; then
    echo "✅ delta: $(delta --version 2>/dev/null | head -1)"
else
    echo "❌ delta: NOT INSTALLED"
fi

echo ""
echo "Git configuration:"
echo "  core.pager: $(git config --global core.pager)"
echo "  delta.side-by-side: $(git config --global delta.side-by-side)"

# ═══════════════════════════════════════════════════════════════════════════════
# COMPLETE
# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  LAYER 3 COMPLETE!                                         ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Next steps:"
echo "1. Authenticate gh CLI: gh auth login"
echo "2. Try lazygit: lazygit"
echo "3. See beautiful diffs: git diff"
echo "4. Run Layer 4: ./scripts/linux/install-layer-4.sh"
echo ""
