#!/usr/bin/env bash
# macOS health check for platform-specific installation and config parity.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos

SUMMARY_MODE=false
STRICT_MODE=false
VERBOSE=true
FAILURES=0
WARNINGS=0
PASSED=0

for arg in "$@"; do
  case "$arg" in
    --summary)
      SUMMARY_MODE=true
      ;;
    --strict)
      STRICT_MODE=true
      ;;
    --help|-h)
      echo "Usage: $0 [--summary] [--strict]"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg"
      exit 1
      ;;
  esac
done

if [[ "$SUMMARY_MODE" == true ]]; then
  VERBOSE=false
fi

log_ok() {
  ((PASSED += 1))
  if [[ "$VERBOSE" == true ]]; then
    echo -e "${GREEN}[OK]${NC} $1"
  fi
}

log_warn_local() {
  ((WARNINGS += 1))
  if [[ "$VERBOSE" == true ]]; then
    echo -e "${YELLOW}[WARN]${NC} $1"
  fi
}

log_fail() {
  ((FAILURES += 1))
  if [[ "$VERBOSE" == true ]]; then
    echo -e "${RED}[FAIL]${NC} $1"
  fi
}

check_command() {
  local cmd="$1"
  shift

  if command_exists "$cmd"; then
    local out
    out="$("$cmd" "$@" 2>/dev/null | head -1 || true)"
    if [[ -n "$out" ]]; then
      log_ok "$cmd: $out"
    else
      log_ok "$cmd: installed"
    fi
  else
    log_fail "$cmd missing"
  fi
}

check_file_parity() {
  local local_file="$1"
  local repo_file="$2"
  local label="$3"

  if [[ ! -f "$local_file" ]]; then
    if [[ "$STRICT_MODE" == true ]]; then
      log_fail "$label missing locally: $local_file"
    else
      log_warn_local "$label missing locally: $local_file"
    fi
    return
  fi

  if cmp --silent "$local_file" "$repo_file"; then
    log_ok "$label parity: OK"
  else
    if [[ "$STRICT_MODE" == true ]]; then
      log_fail "$label parity mismatch"
    else
      log_warn_local "$label parity mismatch"
    fi
  fi
}

if [[ "$VERBOSE" == true ]]; then
  echo ""
  echo "════════════════════════════════════════════════════════════"
  echo "  MACOS HEALTH CHECK"
  echo "  $(date -Iseconds)"
  echo "════════════════════════════════════════════════════════════"
fi

# Syntax checks for macOS scripts
while IFS= read -r -d '' script; do
  if bash -n "$script"; then
    log_ok "bash -n $script"
  else
    log_fail "bash -n $script"
  fi
done < <(find "$SCRIPT_DIR" -name "*.sh" -print0 | sort -z)

# Base requirements
if command_exists brew; then
  log_ok "brew available"
else
  log_fail "brew missing"
fi

check_command rldyourterm-stable --version
check_command fish --version
check_command starship --version

# Layer 1
check_command bat --version
check_command fd --version
check_command rg --version
check_command sd --version
check_command jq --version
check_command yq --version
check_command eza --version

# Layer 2
check_command fzf --version
check_command zoxide --version
check_command atuin --version
check_command uv --version
check_command bun --version
check_command watchexec --version
check_command glow --version
check_command btm --version
check_command hyperfine --version

# Layer 3
check_command gh --version
check_command lazygit --version
check_command delta --version

# Layer 4
check_command ctags --version
check_command tokei --version
if command_exists sg; then
  check_command sg --version
elif command_exists ast-grep; then
  check_command ast-grep --version
else
  log_fail "ast-grep/sg missing"
fi
check_command probe --version
check_command semgrep --version
if command_exists grepai; then
  if grepai version >/dev/null 2>&1; then
    check_command grepai version
  else
    check_command grepai --version
  fi
else
  log_fail "grepai missing"
fi

# Layer 5
check_command node --version
check_command npm --version
check_command claude --version
check_command gemini --version
check_command codex --version

check_file_parity "$HOME/.rldyourterm.lua" "$PROJECT_DIR/configs/rldyourterm/rldyourterm.lua" "rldyourterm config"
check_file_parity "$HOME/.config/fish/config.fish" "$PROJECT_DIR/configs/fish/config.fish" "Fish config"
check_file_parity "$HOME/.config/starship.toml" "$PROJECT_DIR/configs/starship/starship.toml" "Starship config"

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  Health Check Summary"
echo "  Passed:  $PASSED"
echo "  Warnings:$WARNINGS"
echo "  Failures:$FAILURES"
echo "════════════════════════════════════════════════════════════"

if [[ $FAILURES -gt 0 ]]; then
  echo "Health status: FAIL"
  exit 1
fi

if [[ "$STRICT_MODE" == true && $WARNINGS -gt 0 ]]; then
  echo "Health status: FAIL (strict mode treats warnings as failure)"
  exit 1
fi

echo "Health status: PASS"
