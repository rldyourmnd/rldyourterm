# Layer 2: Productivity - Complete Guide

> Speed-focused tools for navigation, history, and development workflow

## Overview

Layer 2 provides productivity tools that dramatically improve daily workflow:

| Tool | Score | Purpose | Key Feature |
|------|-------|---------|-------------|
| **fzf** | 88.7 | Fuzzy finder | Ctrl+R history, Ctrl+T files |
| **zoxide** | 95.5 | Smart cd | Frecency-based navigation |
| **atuin** | 90.5 | History sync | SQLite + encrypted sync |
| **uv** | 94.3 | Python packages | 10-100x faster than pip |
| **bun** | 92.7 | JavaScript runtime | 3x faster than npm |
| **watchexec** | 89.9 | File watcher | Auto-restart on changes |
| **glow** | 88.2 | Markdown renderer | Terminal markdown |
| **bottom** | 91.5 | System monitor | htop alternative |
| **hyperfine** | 94.1 | Benchmarking | Statistical analysis |

---

## Installation

### Quick Install

```bash
./scripts/linux/install-layer-2.sh
```

### Manual Installation

```bash
# fzf (88.7) - fuzzy finder
git clone --depth 1 https://github.com/junegunn/fzf.git ~/.fzf
~/.fzf/install --key-bindings --completion --no-update-rc --no-bash --no-zsh

# zoxide (95.5) - smart cd
cargo install --locked zoxide

# atuin (90.5) - history sync
tmp_atuin="$(mktemp)"
curl --proto '=https' --tlsv1.2 -fsSL https://setup.atuin.sh -o "$tmp_atuin"
sh "$tmp_atuin"
rm -f "$tmp_atuin"

# uv (94.3) - Python package manager
tmp_uv="$(mktemp)"
curl --proto '=https' --tlsv1.2 -fsSL https://astral.sh/uv/install.sh -o "$tmp_uv"
sh "$tmp_uv"
rm -f "$tmp_uv"

# bun (92.7) - JavaScript runtime
tmp_bun="$(mktemp)"
curl --proto '=https' --tlsv1.2 -fsSL https://bun.sh/install -o "$tmp_bun"
bash "$tmp_bun"
rm -f "$tmp_bun"

# watchexec (89.9) - file watcher
cargo install --locked watchexec-cli

# glow (88.2) - markdown renderer (recommended via apt, with GitHub fallback)
if ! command -v glow >/dev/null 2>&1; then
  if command -v apt-cache >/dev/null 2>&1 && apt-cache show glow >/dev/null 2>&1; then
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends glow
  else
    GLOW_VERSION=$(curl -s https://api.github.com/repos/charmbracelet/glow/releases/latest \
      | grep -oP '\"tag_name\": \"\\K(.*)(?=\")' || echo "v2.1.2")
    GLOW_TMP_DIR="$(mktemp -d)"
    GLOW_ASSET_URL="$(curl -s https://api.github.com/repos/charmbracelet/glow/releases/latest \
      | grep -oP '\"browser_download_url\": \"\\K([^\"]+)' \
      | grep -E "glow.*${GLOW_VERSION#v}.*(linux|Linux).*(tar\\.gz)$" \
      | head -n 1)"

    if [ -n "$GLOW_ASSET_URL" ]; then
      curl --proto '=https' --tlsv1.2 -fSsL -o "$GLOW_TMP_DIR/glow.tar.gz" "$GLOW_ASSET_URL"
      tar -xzf "$GLOW_TMP_DIR/glow.tar.gz" -C "$GLOW_TMP_DIR"
      mkdir -p "$HOME/.local/bin"
      if [ -x "$GLOW_TMP_DIR/glow" ]; then
        mv "$GLOW_TMP_DIR/glow" "$HOME/.local/bin/glow"
      elif [ -x "$GLOW_TMP_DIR/bin/glow" ]; then
        mv "$GLOW_TMP_DIR/bin/glow" "$HOME/.local/bin/glow"
      else
        echo "Glow archive did not contain a glow binary"
      fi
      chmod +x "$HOME/.local/bin/glow"
      rm -rf "$GLOW_TMP_DIR"
    else
      echo "Compatible glow archive was not found for fallback install"
    fi
  fi
fi

# bottom (91.5) - system monitor
cargo install --locked bottom

# hyperfine (94.1) - benchmarking
cargo install --locked hyperfine
```

## Usage Examples

### fzf - Fuzzy Finder

```bash
# Find files (Ctrl+T)
# Type to search, Enter to select

# Search command history (Ctrl+R)
# Fuzzy search through all history

# Change directory (Alt+C)
# Fuzzy search directories

# Preview files
fzf --preview 'bat --style=numbers --color=always {}'

# With ripgrep
rg --files | fzf

# Custom command
find * -type f | fzf > selected
```

### zoxide - Smart cd

```bash
# Add directory to database
z /path/to/project

# Jump to frequently used directory
z proj          # Jumps to ~/projects if frequently visited

# Jump with query
z repo code     # Jumps to ~/repos/code-project

# Interactive selection
zi              # fzf-powered selection

# Previous directory
z -
```

### Atuin - History Sync

```bash
# Search history (Ctrl+R replaced)
# Type to search with context

# Sync history
atuin sync

# View history
atuin history list

# Search specific command
atuin search "git commit"

# Login for sync
atuin login -u USERNAME
```

### uv - Python Package Manager

```bash
# Install package
uv pip install package

# Install from requirements
uv pip install -r requirements.txt

# Create virtual environment
uv venv

# Run Python with package
uvx package script.py

# Install tool globally
uv tool install black
```

### bun - JavaScript Runtime

```bash
# Install package
bun add package

# Run script
bun run script.ts

# Start project
bun start

# Install dependencies
bun install

# Run file directly
bun file.ts
```

### watchexec - File Watcher

```bash
# Run on any change
watchexec command

# Watch specific extensions
watchexec -e py,rs command

# Watch specific directory
watchexec -w src command

# Clear screen before run
watchexec -c command

# With shell
watchexec --shell bash "echo 'changed'"
```

### glow - Markdown Renderer

```bash
# Render file
glow README.md

# Render from stdin
cat README.md | glow

# Browse current directory
glow

# Pagination
glow -p README.md
```

### bottom - System Monitor

```bash
# Start monitor
btm

# Basic mode
btm --basic

# No network
btm --hide_avg_cpu
```

### hyperfine - Benchmarking

```bash
# Benchmark command
hyperfine 'command1' 'command2'

# Multiple runs
hyperfine -r 10 'command'

# Export results
hyperfine --export-markdown results.md 'command'
```

## Shell Integration

### Fish

```fish
# fzf
fzf --fish | source

# zoxide
zoxide init fish | source

# Atuin
atuin init fish --disable-up-arrow | source
```

### Zsh

```zsh
# fzf
source <(fzf --zsh)

# zoxide
eval "$(zoxide init zsh)"

# Atuin
eval "$(atuin init zsh)"
```

## Performance Comparison

| Task | Classic | Modern | Speedup |
|------|---------|--------|---------|
| Install Python package | pip | uv | 10-100x |
| Install JS package | npm | bun | 3-10x |
| Directory navigation | cd | zoxide | 5-10x |
| History search | Ctrl+R | Atuin | Better UX |

---

## Troubleshooting

### 1. fzf key bindings not working

**Cause:** Fish integration not sourced.

**Solution:**
```bash
fzf --fish | source
```

Add to `~/.config/fish/config.fish`:
```fish
fzf --fish | source
```

### 2. zoxide: "database not found"

**Cause:** No directories indexed yet.

**Solution:**
```bash
cd ~/projects
cd ~/documents
z projects  # Now works
```

### 3. atuin: "failed to connect to server"

**Cause:** Sync server unreachable (optional feature).

**Solution:**
```bash
# Disable sync (local only)
atuin logout
```

### 4. uv: "command not found"

**Cause:** PATH not updated after installation.

**Solution:**
```bash
source ~/.cargo/env
# or restart shell
exec fish
```

### 5. bun: "command not found"

**Cause:** bun install path not in PATH.

**Solution:**
Add to `~/.config/fish/config.fish`:
```fish
set -gx BUN_INSTALL $HOME/.bun
set -gx PATH $BUN_INSTALL/bin $PATH
```

### 6. bottom: high CPU usage

**Cause:** Default update interval too fast.

**Solution:**
Create `~/.config/bottom/bottom.toml`:
```toml
[dot_marker]
disabled = true

[cpu]
default = "average"
```

---

## Verification

```bash
# Check all tools
for tool in fzf zoxide atuin uv bun watchexec glow btm hyperfine; do
    if command -v $tool &>/dev/null; then
        echo "✅ $tool: $($tool --version 2>/dev/null | head -1)"
    else
        echo "❌ $tool: NOT INSTALLED"
    fi
done
```

---

## Next Steps

After Layer 2 is complete:
- **Layer 3: GitHub & Git** - gh CLI, lazygit, delta

```bash
./scripts/linux/install-layer-3.sh
```
