#!/usr/bin/env bash
# macOS orchestrator: Foundation + Layers 1..5

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

ensure_macos

DRY_RUN=false
for arg in "$@"; do
  case "$arg" in
    --dry-run)
      DRY_RUN=true
      ;;
    --help|-h)
      echo "Usage: $0 [--dry-run]"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg"
      echo "Usage: $0 [--dry-run]"
      exit 1
      ;;
  esac
done

if [[ "$DRY_RUN" == false ]]; then
  ensure_brew
else
  log_info "Dry-run mode: skipping Homebrew checks and package installation"
fi

run_step() {
  local script_path="$1"
  local script_name

  script_name="$(basename "$script_path")"
  if [[ ! -x "$script_path" ]]; then
    log_error "Script missing or not executable: $script_path"
    exit 1
  fi

  log_info "Running $script_name..."
  if [[ "$DRY_RUN" == true ]]; then
    log_info "[dry-run] would execute: $script_path"
    return 0
  fi

  "$script_path"
  log_success "$script_name complete"
}

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  MACOS INSTALLATION PIPELINE"
echo "  Foundation + Layers 1..5"
echo "════════════════════════════════════════════════════════════"
echo ""

run_step "$SCRIPT_DIR/install-foundation.sh"
run_step "$SCRIPT_DIR/install-layer-1.sh"
run_step "$SCRIPT_DIR/install-layer-2.sh"
run_step "$SCRIPT_DIR/install-layer-3.sh"
run_step "$SCRIPT_DIR/install-layer-4.sh"
run_step "$SCRIPT_DIR/install-layer-5.sh"

if [[ "$DRY_RUN" == true ]]; then
  echo ""
  echo -e "${GREEN}Dry-run flow validation PASS${NC}"
  exit 0
fi

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  MACOS INSTALLATION COMPLETE!                              ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Next steps:"
echo "1. Restart terminal and run: exec fish"
echo "2. Authenticate GitHub CLI: gh auth login"
echo "3. Authenticate AI CLIs: claude / gemini / codex"
echo "4. Run health-check: ./scripts/macos/health-check.sh --summary"
