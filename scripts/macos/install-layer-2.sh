#!/usr/bin/env bash
# macOS Layer 2: productivity

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos
ensure_brew

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS LAYER 2: PRODUCTIVITY"
echo "  fzf, zoxide, atuin, uv, bun, watchexec, glow, bottom, hyperfine"
echo "════════════════════════════════════════════════════════════"
echo ""

install_formulae fzf zoxide atuin uv bun watchexec glow bottom hyperfine

# Install fzf key bindings/completions without modifying shell rc files.
FZF_INSTALL="$(brew --prefix)/opt/fzf/install"
if [[ -x "$FZF_INSTALL" ]]; then
  if "$FZF_INSTALL" --key-bindings --completion --no-update-rc --no-bash --no-zsh; then
    log_success "fzf shell assets installed"
  else
    log_warn "fzf shell assets install returned non-zero"
  fi
fi

ensure_local_bin_path_for_zsh

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

print_tool_version fzf fzf --version
print_tool_version zoxide zoxide --version
print_tool_version atuin atuin --version
print_tool_version uv uv --version
print_tool_version bun bun --version
print_tool_version watchexec watchexec --version
print_tool_version glow glow --version
print_tool_version btm btm --version
print_tool_version hyperfine hyperfine --version

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  LAYER 2 COMPLETE                                          ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
