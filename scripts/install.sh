#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# AWESOME TERMINAL FOR AI - Complete Installation Script
# ╀═════════════════════════════════════════════════════════════════════════════
# Orchestrates all installation layers and foundation setup.
# Run: ./scripts/install.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Resolve project root so script works from any current directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

command_exists() { command -v "$1" &>/dev/null; }

to_windows_path_if_needed() {
    local input_path="$1"
    if command_exists cygpath; then
        cygpath -w "$input_path"
    else
        echo "$input_path"
    fi
}

OS_NAME="$(uname -s)"
if [[ "$OS_NAME" == "Darwin" ]]; then
    if [[ -x "$PROJECT_DIR/scripts/macos/install.sh" ]]; then
        log_info "Detected macOS. Delegating to scripts/macos/install.sh"
        exec "$PROJECT_DIR/scripts/macos/install.sh" "$@"
    fi
    log_error "macOS installer is missing: $PROJECT_DIR/scripts/macos/install.sh"
    exit 1
fi

case "$OS_NAME" in
    MINGW*|MSYS*|CYGWIN*)
        if [[ -f "$PROJECT_DIR/scripts/install-windows.ps1" ]]; then
            WINDOWS_SCRIPT_PATH="$(to_windows_path_if_needed "$PROJECT_DIR/scripts/install-windows.ps1")"
            PS_ARGS=()
            for arg in "$@"; do
                case "$arg" in
                    --dry-run)
                        PS_ARGS+=("-DryRun")
                        ;;
                    --help|-h)
                        PS_ARGS+=("-Help")
                        ;;
                    *)
                        PS_ARGS+=("$arg")
                        ;;
                esac
            done
            if command_exists pwsh; then
                log_info "Detected Windows-compatible shell environment. Delegating to install-windows.ps1 via pwsh"
                exec pwsh -NoProfile -ExecutionPolicy Bypass -File "$WINDOWS_SCRIPT_PATH" "${PS_ARGS[@]}"
            fi
            if command_exists powershell.exe; then
                log_info "Detected Windows-compatible shell environment. Delegating to install-windows.ps1 via powershell.exe"
                exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$WINDOWS_SCRIPT_PATH" "${PS_ARGS[@]}"
            fi
            log_error "Neither pwsh nor powershell.exe found in PATH."
            exit 1
        fi
        log_error "Windows installer is missing: $PROJECT_DIR/scripts/install-windows.ps1"
        exit 1
        ;;
esac

if [[ "$OS_NAME" != "Linux" ]]; then
    log_error "Unsupported OS: $OS_NAME"
    log_error "Supported now: Linux (Debian/Ubuntu), macOS, Windows (PowerShell)"
    exit 1
fi

print_usage() {
    cat <<'EOF'
Usage: ./scripts/install.sh [--dry-run] [--help]
  --dry-run  Validate Linux flow without running installers
  --help     Show this help
EOF
}

DRY_RUN=false
for arg in "$@"; do
    case "$arg" in
        --dry-run)
            DRY_RUN=true
            ;;
        --help|-h)
            print_usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $arg"
            print_usage
            exit 1
            ;;
    esac
done

run_step() {
    local script_path="$1"
    local name

    name="$(basename "$script_path")"
    if [ ! -x "$script_path" ]; then
        log_error "Script missing or not executable: $script_path"
        return 1
    fi

    log_info "Running ${name}..."
    if [[ "$DRY_RUN" == true ]]; then
        log_info "[dry-run] would execute: $script_path"
    else
        "$script_path"
    fi
    log_success "${name} complete"
}

log_info "Running preflight checks..."

for cmd in curl git; do
    if ! command_exists "$cmd"; then
        log_error "$cmd is required but not installed"
        exit 1
    fi

done

log_success "Preflight checks passed"
log_info "Starting full installation pipeline"

run_step "$PROJECT_DIR/scripts/linux/install-foundation.sh"
run_step "$PROJECT_DIR/scripts/linux/install-layer-1.sh"
run_step "$PROJECT_DIR/scripts/linux/install-layer-2.sh"
run_step "$PROJECT_DIR/scripts/linux/install-layer-3.sh"
run_step "$PROJECT_DIR/scripts/linux/install-layer-4.sh"
run_step "$PROJECT_DIR/scripts/linux/install-layer-5.sh"

if [[ "$DRY_RUN" == true ]]; then
    log_success "Linux dry-run flow validation PASS"
    exit 0
fi

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  ALL 5 LAYERS + FOUNDATION COMPLETE!                        ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Layers installed:"
echo "  Foundation: rldyourterm + Fish + Starship"
echo "  Layer 1:    bat, fd, rg, sd, jq, yq, eza"
echo "  Layer 2:    fzf, zoxide, atuin, uv, bun, watchexec, glow, bottom, hyperfine"
echo "  Layer 3:    gh CLI, lazygit, delta"
echo "  Layer 4:    grepai, ast-grep, probe, semgrep, ctags, tokei"
echo "  Layer 5:    Claude Code, Gemini CLI, Codex CLI"
echo ""
echo "Next steps:"
echo "1. Log out and back in (or reboot) to activate Fish as default shell"
echo "2. Initialize grepai: grepai init"
echo "3. Login to GitHub CLI: gh auth login"
echo "4. Login to Atuin (optional): atuin login"
echo "5. Authenticate AI CLIs:"
echo "   - claude (browser auth opens automatically)"
echo "   - gemini (select 'Login with Google')"
echo "   - codex (export OPENAI_API_KEY='your-key')"
echo ""
echo "Enjoy your Ultimate AI Terminal Environment!"
