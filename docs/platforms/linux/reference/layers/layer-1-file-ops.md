# Layer 1: File Operations

> Ultra-fast file manipulation tools replacing classic Unix utilities

## Overview

This layer provides 10-100x faster alternatives to traditional file operations. All tools are written in Rust or Go for maximum performance.

## Tools

| Tool | Score | Replaces | Description |
|------|-------|----------|-------------|
| **bat** | 91.8 | cat | Syntax highlighting + Git integration |
| **fd** | 86.1 | find | Parallel traversal, smart defaults |
| **rg** | 81 | grep | Respects gitignore, 10x+ faster |
| **sd** | 90.8 | sed | Painless regex, intuitive syntax |
| **jq** | 85.7 | - | JSON processor and query tool |
| **yq** | 96.4 | - | YAML/JSON/XML processor |
| **eza** | - | ls | Modern, colorful, git-aware |

## Installation

```bash
# bat (91.8) - cat replacement
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends bat
# Create symlink (Ubuntu names it batcat)
mkdir -p ~/.local/bin && ln -sf /usr/bin/batcat ~/.local/bin/bat

# jq (85.7) - JSON processor
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends jq

# fd (86.1) - find replacement
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends fd-find
ln -sf $(which fdfind) ~/.local/bin/fd

# eza - ls replacement
if apt-cache show eza >/dev/null 2>&1; then
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends eza
else
  cargo install --locked eza
fi

# sd (90.8) - sed replacement
cargo install --locked sd

# yq (96.4) - YAML/JSON/XML processor
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64) ARCH=amd64 ;;
  aarch64|arm64|armv7l|armv7*) ARCH=arm64 ;;
  *) ARCH=amd64 ;;
esac
mkdir -p ~/.local/bin
curl --proto '=https' --tlsv1.2 -fSsL -o ~/.local/bin/yq "https://github.com/mikefarah/yq/releases/latest/download/yq_linux_${ARCH}"
chmod +x ~/.local/bin/yq
# Optional pinning:
# export YQ_SHA256="<sha256-from-official-release>"
# echo "${YQ_SHA256}  ~/.local/bin/yq" | sha256sum -c -

# ripgrep (81) - grep replacement
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ripgrep
```

## Usage Examples

### bat - cat with superpowers

```bash
# Basic usage
bat file.txt

# With line numbers only
bat -n file.txt

# Show Git changes
bat --diff file.txt

# Multiple files
bat file1.txt file2.py file3.rs

# Specify language
bat -l python script.py
```

### fd - find replacement

```bash
# Find by name
fd pattern

# Find files only
fd -t f pattern

# Find directories only
fd -t d pattern

# Find and execute
fd -e rs -x wc -l

# Exclude directories
fd pattern --exclude node_modules --exclude target
```

### rg - ripgrep

```bash
# Basic search
rg pattern

# Search specific files
rg -t rust pattern

# Search with context
rg -C 3 pattern

# Search and replace preview
rg -r 'replacement' pattern

# List matching files only
rg -l pattern
```

### sd - sed replacement

```bash
# Simple replacement
sd 'old' 'new' file.txt

# Regex replacement
sd '(\w+)@(\w+)' '$2@$1' file.txt

# In-place edit
sd -i 'old' 'new' file.txt

# Pipeline
cat file.txt | sd 'old' 'new'
```

### jq - JSON processor

```bash
# Pretty print
jq '.' file.json

# Extract field
jq '.field' file.json

# Filter array
jq '.[] | select(.age > 30)' file.json

# Transform
jq '{name: .name, age: .age}' file.json
```

### yq - YAML/JSON/XML processor

```bash
# Read YAML
yq '.key' file.yaml

# Convert YAML to JSON
yq -o=json '.' file.yaml

# Update value
yq '.key = "new value"' -i file.yaml

# Read XML
yq -p=xml '.root.element' file.xml
```

## Performance Benchmarks

| Operation | Classic Tool | Modern Tool | Speedup |
|-----------|--------------|-------------|---------|
| File search | find | fd | 10x+ |
| Text search | grep | rg | 10x+ |
| File listing | ls | eza | 2x |
| Text replace | sed | sd | 5x+ |

## Aliases (add to shell config)

```bash
# cat -> bat
alias cat='bat'

# ls -> eza
alias ls='eza'
alias ll='eza -la'
alias lt='eza --tree'

# find -> fd
alias find='fd'

# grep -> rg (already default in many systems)
```
