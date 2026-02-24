#!/usr/bin/env bash
# macOS Layer 5: AI orchestration CLIs

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos
ensure_brew
ensure_node_npm

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS LAYER 5: AI ORCHESTRATION"
echo "  Claude Code, Gemini CLI, Codex CLI"
echo "════════════════════════════════════════════════════════════"
echo ""

install_npm_global() {
  local binary="$1"
  local package="$2"

  if command_exists "$binary"; then
    log_info "$binary already installed"
    return 0
  fi

  log_info "Installing $package..."
  npm install -g "$package"
  log_success "$package installed"
}

install_npm_global claude @anthropic-ai/claude-code
install_npm_global gemini @google/gemini-cli
install_npm_global codex @openai/codex

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

print_tool_version node node --version
print_tool_version npm npm --version
print_tool_version claude claude --version
print_tool_version gemini gemini --version
print_tool_version codex codex --version

echo ""
echo "Authentication:"
echo "- claude   -> run: claude"
echo "- gemini   -> run: gemini (OAuth) or export GEMINI_API_KEY"
echo "- codex    -> export OPENAI_API_KEY and run: codex"

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  LAYER 5 COMPLETE                                          ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
