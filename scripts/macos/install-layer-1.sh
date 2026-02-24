#!/usr/bin/env bash
# macOS Layer 1: file operations

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos
ensure_brew

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS LAYER 1: FILE OPERATIONS"
echo "  bat, fd, rg, sd, jq, yq, eza"
echo "════════════════════════════════════════════════════════════"
echo ""

install_formulae bat fd ripgrep sd jq yq eza
ensure_local_bin_path_for_zsh

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

print_tool_version bat bat --version
print_tool_version fd fd --version
print_tool_version rg rg --version
print_tool_version sd sd --version
print_tool_version jq jq --version
print_tool_version yq yq --version
print_tool_version eza eza --version

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  LAYER 1 COMPLETE                                          ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
