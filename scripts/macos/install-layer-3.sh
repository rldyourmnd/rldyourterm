#!/usr/bin/env bash
# macOS Layer 3: GitHub & Git workflow

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos
ensure_brew

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS LAYER 3: GITHUB & GIT"
echo "  gh, lazygit, delta"
echo "════════════════════════════════════════════════════════════"
echo ""

install_formulae gh lazygit git-delta
ensure_local_bin_path_for_zsh

# Catppuccin theme for bat (used by delta syntax theme)
BAT_THEMES_DIR="$(bat --config-dir 2>/dev/null || true)/themes"
if [[ -n "$BAT_THEMES_DIR" ]]; then
  mkdir -p "$BAT_THEMES_DIR"
  if [[ ! -f "$BAT_THEMES_DIR/Catppuccin Mocha.tmTheme" ]]; then
    if curl -fsSL "https://github.com/catppuccin/bat/raw/main/themes/Catppuccin%20Mocha.tmTheme" \
      -o "$BAT_THEMES_DIR/Catppuccin Mocha.tmTheme"; then
      bat cache --build >/dev/null 2>&1 || true
      log_success "Installed Catppuccin Mocha theme for bat"
    else
      log_warn "Could not download Catppuccin Mocha theme for bat"
    fi
  fi
fi

# Git defaults for delta workflow
git config --global core.pager "delta"
git config --global delta.line-numbers true
git config --global delta.side-by-side true
git config --global delta.navigate true
git config --global delta.dark true
git config --global delta.syntax-theme "Catppuccin Mocha" 2>/dev/null || \
  git config --global delta.syntax-theme "Monokai Extended"
git config --global delta.hyperlinks true
git config --global delta.hyperlinks-commit-link-format "https://github.com/{host}/{org}/{repo}/commit/{commit}"
git config --global interactive.diffFilter "delta --color-only"
git config --global merge.conflictstyle diff3
git config --global diff.colorMoved default

git config --global alias.co checkout
git config --global alias.br branch
git config --global alias.ci commit
git config --global alias.st status
git config --global alias.unstage 'reset HEAD --'
git config --global alias.last 'log -1 HEAD'
git config --global alias.visual '!lazygit'
git config --global alias.lg 'log --oneline --graph --all'

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

print_tool_version gh gh --version
print_tool_version lazygit lazygit --version
print_tool_version delta delta --version

echo ""
echo "Git configuration:"
echo "  core.pager: $(git config --global core.pager || echo unset)"
echo "  delta.side-by-side: $(git config --global delta.side-by-side || echo unset)"

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  LAYER 3 COMPLETE                                          ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
