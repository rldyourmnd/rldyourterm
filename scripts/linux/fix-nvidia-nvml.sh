#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# NVIDIA NVML RECOVERY (Linux)
# ═══════════════════════════════════════════════════════════════════════════════
# Fixes common "nvidia-smi: Failed to initialize NVML: Unknown Error" cases
# by starting nvidia-persistenced and (optionally) registering it on boot.
# Run: ./scripts/linux/fix-nvidia-nvml.sh [--dry-run] [--no-boot-persist]

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

command_exists() { command -v "$1" &>/dev/null; }

usage() {
    cat <<'EOF'
Usage: ./scripts/linux/fix-nvidia-nvml.sh [--dry-run] [--no-boot-persist] [--help]
  --dry-run          Print planned actions without changing the system
  --no-boot-persist  Do not register nvidia-persistenced for multi-user target
  --help             Show this help
EOF
}

DRY_RUN=false
BOOT_PERSIST=true

for arg in "$@"; do
    case "$arg" in
        --dry-run)
            DRY_RUN=true
            ;;
        --no-boot-persist)
            BOOT_PERSIST=false
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $arg"
            usage
            exit 1
            ;;
    esac
done

run_cmd() {
    if [[ "$DRY_RUN" == true ]]; then
        printf '[dry-run]'
        printf ' %q' "$@"
        echo
    else
        "$@"
    fi
}

echo ""
echo "════════════════════════════════════════════════════════════"
echo "  NVIDIA NVML RECOVERY (Linux)"
echo "════════════════════════════════════════════════════════════"
echo ""

if [[ "$(uname -s)" != "Linux" ]]; then
    log_error "This script supports Linux only."
    exit 1
fi

if ! command_exists nvidia-smi; then
    log_error "nvidia-smi not found. Install NVIDIA driver userspace tools first."
    exit 1
fi

if ! command_exists systemctl; then
    log_error "systemctl not found. Cannot manage nvidia-persistenced."
    exit 1
fi

log_info "Checking current NVML state..."
if nvidia-smi >/dev/null 2>&1; then
    log_success "nvidia-smi already works."
else
    log_warn "nvidia-smi currently fails. Applying recovery steps."
fi

if systemctl is-active --quiet nvidia-persistenced 2>/dev/null; then
    log_info "nvidia-persistenced is already active."
else
    log_info "Starting nvidia-persistenced..."
    run_cmd sudo systemctl start nvidia-persistenced
fi

if [[ "$BOOT_PERSIST" == true ]]; then
    log_info "Ensuring nvidia-persistenced is pulled by multi-user.target..."
    run_cmd sudo systemctl add-wants multi-user.target nvidia-persistenced.service
fi

if [[ "$DRY_RUN" == true ]]; then
    log_info "Dry-run complete."
    exit 0
fi

log_info "Verifying NVML/nvidia-smi..."
if nvidia-smi >/dev/null 2>&1; then
    log_success "nvidia-smi is operational."
    nvidia-smi --query-gpu=name,driver_version,utilization.gpu,memory.used,memory.total --format=csv,noheader
else
    log_error "nvidia-smi still fails."
    log_error "Collect diagnostics with: journalctl -k -b | rg -i 'nvidia|nvrm|xid|nvml'"
    exit 1
fi
