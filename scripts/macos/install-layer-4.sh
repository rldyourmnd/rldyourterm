#!/usr/bin/env bash
# macOS Layer 4: code intelligence

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos
ensure_brew

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS LAYER 4: CODE INTELLIGENCE"
echo "  grepai, ast-grep, probe, semgrep, ctags, tokei"
echo "════════════════════════════════════════════════════════════"
echo ""

install_formulae universal-ctags tokei ast-grep semgrep

# probe-code: cargo path (Homebrew has no stable formula guarantee)
if ! command_exists probe; then
  ensure_cargo
  log_info "Installing probe via cargo (probe-code)..."
  cargo install --locked probe-code
  log_success "probe installed"
else
  log_info "probe already installed"
fi

# grepai: try Homebrew first, fallback to GitHub release binary
if ! command_exists grepai; then
  if brew info grepai >/dev/null 2>&1; then
    install_formulae grepai
  else
    log_info "grepai formula not available; installing from GitHub release"

    RELEASE_JSON="$(curl -fsSL https://api.github.com/repos/yoanbernabeu/grepai/releases/latest || true)"
    if [[ -z "$RELEASE_JSON" ]]; then
      log_error "Could not fetch grepai release metadata"
      exit 1
    fi

    ARCH="$(arch_for_release)"
    ASSET_URL="$(printf '%s' "$RELEASE_JSON" \
      | grep -oE '"browser_download_url": "[^"]+"' \
      | cut -d'"' -f4 \
      | grep -E "grepai_.*darwin_${ARCH}.*(tar\.gz|zip)$" \
      | head -n1)"

    if [[ -z "$ASSET_URL" ]]; then
      ASSET_URL="$(printf '%s' "$RELEASE_JSON" \
        | grep -oE '"browser_download_url": "[^"]+"' \
        | cut -d'"' -f4 \
        | grep -Ei 'darwin.*(tar\.gz|zip)$' \
        | head -n1)"
    fi

    if [[ -z "$ASSET_URL" ]]; then
      log_error "Could not find a compatible macOS grepai release asset"
      exit 1
    fi

    TMP_DIR="$(mktemp -d -t grepai-macos-XXXXXX)"
    ARCHIVE_PATH="$TMP_DIR/grepai-asset"
    mkdir -p "$HOME/.local/bin"

    curl -fsSL "$ASSET_URL" -o "$ARCHIVE_PATH"

    if [[ "$ASSET_URL" == *.tar.gz ]]; then
      tar -xzf "$ARCHIVE_PATH" -C "$TMP_DIR"
    elif [[ "$ASSET_URL" == *.zip ]]; then
      unzip -q "$ARCHIVE_PATH" -d "$TMP_DIR"
    else
      log_error "Unsupported grepai asset format: $ASSET_URL"
      rm -rf "$TMP_DIR"
      exit 1
    fi

    GREPAI_BIN="$(find "$TMP_DIR" -type f -name grepai | head -n1)"
    if [[ -z "$GREPAI_BIN" ]]; then
      log_error "grepai binary not found in downloaded archive"
      rm -rf "$TMP_DIR"
      exit 1
    fi

    install -m 0755 "$GREPAI_BIN" "$HOME/.local/bin/grepai"
    rm -rf "$TMP_DIR"
    log_success "grepai installed in ~/.local/bin"
  fi
else
  log_info "grepai already installed"
fi

ensure_local_bin_path_for_zsh

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  VERIFICATION"
echo "════════════════════════════════════════════════════════════"
echo ""

print_tool_version ctags ctags --version
print_tool_version tokei tokei --version

if command_exists sg; then
  print_tool_version sg sg --version
elif command_exists ast-grep; then
  print_tool_version ast-grep ast-grep --version
else
  echo "❌ ast-grep/sg: NOT INSTALLED"
fi

print_tool_version probe probe --version
print_tool_version semgrep semgrep --version

if command_exists grepai; then
  if grepai version >/dev/null 2>&1; then
    print_tool_version grepai grepai version
  else
    print_tool_version grepai grepai --version
  fi
else
  echo "❌ grepai: NOT INSTALLED"
fi

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  LAYER 4 COMPLETE                                          ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
