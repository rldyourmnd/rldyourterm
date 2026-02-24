#!/usr/bin/env bash
# Shared helpers for macOS install scripts.

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

command_exists() { command -v "$1" >/dev/null 2>&1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

ensure_macos() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    log_error "This installer is for macOS only. Current OS: $(uname -s)"
    exit 1
  fi
}

brew_bin_path() {
  if command_exists brew; then
    command -v brew
    return 0
  fi

  if [[ -x /opt/homebrew/bin/brew ]]; then
    echo /opt/homebrew/bin/brew
    return 0
  fi

  if [[ -x /usr/local/bin/brew ]]; then
    echo /usr/local/bin/brew
    return 0
  fi

  return 1
}

load_brew_shellenv() {
  local brew_bin
  brew_bin="$(brew_bin_path || true)"
  if [[ -n "$brew_bin" ]]; then
    eval "$("$brew_bin" shellenv)"
  fi
}

persist_brew_shellenv() {
  local target="$HOME/.zprofile"
  local line='eval "$(brew shellenv)"'

  touch "$target"
  if ! grep -Fq "$line" "$target"; then
    {
      echo ""
      echo "# Homebrew"
      echo "$line"
    } >> "$target"
    log_success "Persisted Homebrew shellenv in $target"
  fi
}

ensure_brew() {
  ensure_macos

  if command_exists brew; then
    load_brew_shellenv
    return 0
  fi

  log_warn "Homebrew not found. Installing Homebrew..."
  NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

  if ! command_exists brew; then
    load_brew_shellenv
  fi

  if ! command_exists brew; then
    log_error "Homebrew installation failed or brew not on PATH."
    log_error "Open a new shell and rerun this script."
    exit 1
  fi

  load_brew_shellenv
  persist_brew_shellenv
  log_success "Homebrew is ready"
}

brew_formula_installed() {
  brew list --formula "$1" >/dev/null 2>&1
}

brew_cask_installed() {
  brew list --cask "$1" >/dev/null 2>&1
}

install_formulae() {
  local missing=()
  local pkg

  for pkg in "$@"; do
    if ! brew_formula_installed "$pkg"; then
      missing+=("$pkg")
    fi
  done

  if [[ ${#missing[@]} -eq 0 ]]; then
    log_info "All formulae already installed"
    return 0
  fi

  log_info "Installing formulae: ${missing[*]}"
  brew install "${missing[@]}"
  log_success "Formulae installed"
}

install_casks() {
  local missing=()
  local cask

  for cask in "$@"; do
    if ! brew_cask_installed "$cask"; then
      missing+=("$cask")
    fi
  done

  if [[ ${#missing[@]} -eq 0 ]]; then
    log_info "All casks already installed"
    return 0
  fi

  log_info "Installing casks: ${missing[*]}"
  brew install --cask "${missing[@]}"
  log_success "Casks installed"
}

ensure_cargo() {
  if command_exists cargo; then
    return 0
  fi

  log_info "cargo not found. Installing Rust toolchain via Homebrew..."
  install_formulae rust

  if ! command_exists cargo; then
    load_brew_shellenv
  fi

  if ! command_exists cargo; then
    log_error "cargo is not available after installing rust"
    exit 1
  fi
}

ensure_node_npm() {
  if command_exists node && command_exists npm; then
    return 0
  fi

  log_info "Node.js/npm not found. Installing via Homebrew..."
  install_formulae node

  if ! command_exists node || ! command_exists npm; then
    log_error "node/npm not available after installation"
    exit 1
  fi
}

ensure_local_bin_path_for_zsh() {
  local target="$HOME/.zprofile"
  local line='export PATH="$HOME/.local/bin:$PATH"'

  touch "$target"
  if ! grep -Fq "$line" "$target"; then
    {
      echo ""
      echo "# User local binaries"
      echo "$line"
    } >> "$target"
    log_success "Added ~/.local/bin to $target"
  fi
}

arch_for_release() {
  case "$(uname -m)" in
    arm64|aarch64)
      echo arm64
      ;;
    x86_64|amd64)
      echo x86_64
      ;;
    *)
      echo x86_64
      ;;
  esac
}

print_tool_version() {
  local tool="$1"
  shift

  if command_exists "$tool"; then
    local output
    output="$($@ 2>/dev/null | head -1 || true)"
    if [[ -n "$output" ]]; then
      echo "✅ $tool: $output"
    else
      echo "✅ $tool: installed"
    fi
  else
    echo "❌ $tool: NOT INSTALLED"
  fi
}
