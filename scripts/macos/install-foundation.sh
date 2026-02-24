#!/usr/bin/env bash
# macOS Foundation: rldyourterm + Fish + Starship + configs

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos
ensure_brew
persist_brew_shellenv

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS FOUNDATION"
echo "  rldyourterm + Fish + Starship"
echo "════════════════════════════════════════════════════════════"
echo ""

# Toolchain
if ! install_casks rldyourterm; then
  log_warn "rldyourterm cask unavailable"
  exit 1
fi
install_formulae fish starship

# Nerd Font for terminal icons (best-effort)
if ! brew_cask_installed font-jetbrains-mono-nerd-font; then
  log_info "Installing Nerd Font cask for prompt/icon consistency..."
  if ! brew tap | grep -qx 'homebrew/cask-fonts'; then
    brew tap homebrew/cask-fonts
  fi
  if brew install --cask font-jetbrains-mono-nerd-font; then
    log_success "Nerd Font installed"
  else
    log_warn "Nerd Font cask install failed; continuing without blocking"
  fi
else
  log_info "Nerd Font already installed"
fi

# Ensure local config dirs exist
mkdir -p "$HOME/.config/fish"
mkdir -p "$HOME/.config/starship/profiles"
mkdir -p "$HOME/.config/rldyourterm"
mkdir -p "$HOME/.local/bin"

# Apply repository configs
if [[ -f "$PROJECT_DIR/configs/rldyourterm/rldyourterm.lua" ]]; then
  cp "$PROJECT_DIR/configs/rldyourterm/rldyourterm.lua" "$HOME/.rldyourterm.lua"
  cp "$PROJECT_DIR/configs/rldyourterm/rldyourterm.lua" "$HOME/.config/rldyourterm/rldyourterm.lua"
  log_success "rldyourterm config applied"
else
  log_warn "Missing repo rldyourterm config: $PROJECT_DIR/configs/rldyourterm/rldyourterm.lua"
fi

if [[ -f "$PROJECT_DIR/configs/fish/config.fish" ]]; then
  cp "$PROJECT_DIR/configs/fish/config.fish" "$HOME/.config/fish/config.fish"
  log_success "Fish config applied"
else
  log_warn "Missing repo Fish config: $PROJECT_DIR/configs/fish/config.fish"
fi

if [[ -f "$PROJECT_DIR/configs/starship/starship.toml" ]]; then
  cp "$PROJECT_DIR/configs/starship/starship.toml" "$HOME/.config/starship.toml"
  cp "$PROJECT_DIR"/configs/starship/profiles/*.toml "$HOME/.config/starship/profiles/"
  cp "$PROJECT_DIR/scripts/shared/starship/switch-profile.sh" "$HOME/.local/bin/starship-profile"
  chmod +x "$HOME/.local/bin/starship-profile"
  if "$HOME/.local/bin/starship-profile" ultra-max >/dev/null 2>&1; then
    log_success "Starship config applied with ultra-max profile"
  else
    log_warn "Starship profile switcher could not apply ultra-max automatically"
  fi
else
  log_warn "Missing repo Starship config: $PROJECT_DIR/configs/starship/starship.toml"
fi

# Ensure fish is a valid login shell and optionally set default shell.
FISH_PATH="$(command -v fish || true)"
if [[ -n "$FISH_PATH" ]]; then
  if ! grep -qxF "$FISH_PATH" /etc/shells 2>/dev/null; then
    log_info "Adding fish shell path to /etc/shells..."
    if echo "$FISH_PATH" | sudo tee -a /etc/shells >/dev/null; then
      log_success "Added fish to /etc/shells"
    else
      log_warn "Could not update /etc/shells automatically"
    fi
  fi

  if [[ "${SHELL:-}" != "$FISH_PATH" ]]; then
    log_info "Setting fish as default shell..."
    if chsh -s "$FISH_PATH"; then
      log_success "Default shell changed to fish"
    else
      log_warn "Could not change default shell automatically"
    fi
  else
    log_info "Fish is already the default shell"
  fi
fi

# PATH helpers
ensure_local_bin_path_for_zsh

# rldyourterm CLI path helper (for systems where cask app binary is not on PATH)
term_app_dir="/Applications/rldyourterm.app"
if ! command_exists rldyourterm && [[ -x "$term_app_dir/Contents/MacOS/rldyourterm" ]]; then
  ln -sf "$term_app_dir/Contents/MacOS/rldyourterm" "$HOME/.local/bin/rldyourterm"
  log_success "Linked rldyourterm binary into ~/.local/bin"
fi

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

print_tool_version rldyourterm-stable rldyourterm-stable --version
print_tool_version fish fish --version
print_tool_version starship starship --version

echo ""
[[ -f "$HOME/.rldyourterm.lua" ]] && echo "✅ ~/.rldyourterm.lua" || echo "❌ ~/.rldyourterm.lua"
[[ -f "$HOME/.config/rldyourterm/rldyourterm.lua" ]] && echo "✅ ~/.config/rldyourterm/rldyourterm.lua" || echo "❌ ~/.config/rldyourterm/rldyourterm.lua"
[[ -f "$HOME/.config/fish/config.fish" ]] && echo "✅ ~/.config/fish/config.fish" || echo "❌ ~/.config/fish/config.fish"
[[ -f "$HOME/.config/starship.toml" ]] && echo "✅ ~/.config/starship.toml" || echo "❌ ~/.config/starship.toml"

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  FOUNDATION COMPLETE                                       ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
