# Layer 1: File Operations - Complete Guide

> Ultra-fast file manipulation tools that replace classic Unix utilities

## Overview

Layer 1 provides modern, high-performance replacements for classic Unix file tools. All tools are written in Rust or Go and offer 10-100x performance improvements.

| Tool | Score | Replaces | Speed Improvement |
|------|-------|----------|-------------------|
| **bat** | 91.8 | cat | +syntax highlighting, Git integration |
| **fd** | 86.1 | find | parallel traversal, smart defaults |
| **rg** | 81 | grep | respects gitignore, 10x+ faster |
| **sd** | 90.8 | sed | painless regex, intuitive syntax |
| **jq** | 85.7 | - | JSON processor |
| **yq** | 96.4 | - | YAML/JSON/XML processor |
| **eza** | - | ls | modern, colorful, git-aware |

---

## Installation

### Quick Install

```bash
./scripts/linux/install-layer-1.sh
```

### Manual Installation

#### 1. apt packages
```bash
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends bat fd-find ripgrep jq
if apt-cache show eza >/dev/null 2>&1; then
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends eza
else
  cargo install --locked eza
fi
```

#### 2. Create symlinks (Ubuntu-specific)
```bash
# bat -> batcat
mkdir -p ~/.local/bin
ln -sf /usr/bin/batcat ~/.local/bin/bat

# fd -> fdfind
ln -sf /usr/bin/fdfind ~/.local/bin/fd
```

#### 3. sd (sed replacement)
```bash
cargo install --locked sd
```

#### 4. yq (YAML/JSON/XML processor)
```bash
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
```

#### 5. Ensure PATH includes ~/.local/bin
```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

---

## Tool Details

### bat (Score: 91.8)

**Replaces:** `cat`

**Features:**
- Syntax highlighting for 150+ languages
- Git integration (shows modifications)
- Line numbers
- Non-printable character visualization

**Usage:**
```bash
# Basic usage
bat file.py

# Show all characters
bat -A file.txt

# Git diff integration
bat --diff file.py

# Multiple files with headers
bat file1.py file2.py

# Specific language
bat -l python script

# No decorations
bat --plain file.txt
```

**Config location:** `~/.config/bat/config`

---

### fd (Score: 86.1)

**Replaces:** `find`

**Features:**
- Parallel directory traversal
- Respects .gitignore by default
- Smart case sensitivity
- Colorized output
- Regex and glob support

**Usage:**
```bash
# Find by name (regex pattern)
fd pattern

# Only Python files
fd -e py search_term

# Include hidden files
fd -H pattern

# Only directories
fd -t d directory_name

# Only files
fd -t f file_name

# Specific extension
fd -e md

# Exclude patterns
fd -E node_modules pattern

# Execute command on results
fd -e py -x wc -l

# Execute command with all results
fd -e py -X black

# Max depth
fd -d 2 pattern

# Full path search
fd -p path_pattern
```

---

### rg (ripgrep) (Score: 81)

**Replaces:** `grep`

**Features:**
- 10x+ faster than grep
- Respects .gitignore automatically
- Smart case matching
- 24-bit color output
- Supports PCRE2 regex

**Usage:**
```bash
# Basic search
rg pattern

# Case insensitive
rg -i pattern

# Specific file type
rg -t py 'def.*function'
rg -T js pattern  # Exclude JS files

# Show context
rg -C 3 pattern    # 3 lines before and after
rg -B 2 pattern    # 2 lines before
rg -A 2 pattern    # 2 lines after

# Only show filenames
rg -l pattern

# Count matches
rg -c pattern

# Search hidden files
rg --hidden pattern

# Disable gitignore
rg --no-ignore pattern

# Replace in output (preview only)
rg 'old' -r 'new'

# Use capture groups
rg '(\w+)@' -r '$1!'

# List file types
rg --type-list
```

---

### sd (Score: 90.8)

**Replaces:** `sed`

**Features:**
- Intuitive syntax
- Literal string mode (no escaping)
- Regex with capture groups
- In-place editing

**Usage:**
```bash
# Simple replace (literal)
sd 'old' 'new' file.txt

# Regex replace
sd 'from \w+' 'to $0' file.txt

# Capture groups
sd '(\w+)@(\w+)' '$2@$1' file.txt

# In-place edit
sd -p 'old' 'new' file.txt

# Case insensitive
sd -i 'pattern' 'replace' file.txt

# Multiple files
sd 'old' 'new' *.txt

# Read from stdin
echo "hello world" | sd 'world' 'there'
```

**Comparison with sed:**
```bash
# sed (complex escaping)
sed -i 's/\[/\\[/g' file.txt

# sd (simple)
sd -p '[' '\[' file.txt
```

---

### jq (Score: 85.7)

**Purpose:** JSON processor

**Features:**
- Powerful query language
- Streaming support
- Built-in functions
- Formatting output

**Usage:**
```bash
# Pretty print
jq '.' file.json

# Get specific field
jq '.name' file.json

# Nested access
jq '.user.address.city' file.json

# Array access
jq '.items[0]' file.json
jq '.items[]' file.json      # All items
jq '.items | length' file.json

# Filter array
jq '.items[] | select(.price > 100)' file.json

# Map values
jq '.items[] | .price * 1.1' file.json

# Create new object
jq '{name: .name, count: (.items | length)}' file.json

# From stdin
curl -s api.example.com/data.json | jq '.results'
```

---

### yq (Score: 96.4)

**Purpose:** YAML/JSON/XML processor

**Features:**
- jq-like syntax
- Supports YAML, JSON, XML, CSV, TOML
- In-place editing
- Format conversion

**Usage:**
```bash
# Read YAML value
yq '.name' file.yaml

# Pretty print
yq '.' file.yaml

# Update value
yq '.name = "new value"' file.yaml

# In-place edit
yq -i '.version = "2.0"' file.yaml

# Convert YAML to JSON
yq -o=json '.' file.yaml

# Convert JSON to YAML
yq -p=json '.' file.json

# Read array
yq '.items[]' file.yaml

# Nested access
yq '.server.port' config.yaml

# XML support
yq -p=xml '.' file.xml

# Multiple files
yq '.version' *.yaml
```

---

### eza

**Replaces:** `ls`

**Features:**
- Colorized output
- Git status integration
- Icons (optional)
- Tree view
- Hyperlinks

**Usage:**
```bash
# Basic listing
eza

# Long format
eza -l

# All files including hidden
eza -la

# Tree view
eza --tree

# Tree with depth
eza --tree --level=2

# Git status
eza --git

# With icons (requires Nerd Font)
eza --icons

# Sort by size
eza -s size

# Sort by time
eza -s modified

# Only directories
eza -D

# Only files
eza -f

# Full path
eza --absolute

# Combination
eza -la --git --icons
```

**Recommended aliases:**
```bash
alias ls='eza'
alias ll='eza -la'
alias lt='eza --tree --level=2'
alias la='eza -a'
```

---

## Troubleshooting

### 1. "command not found: bat"

**Cause:** Ubuntu installs bat as `batcat`.

**Solution:**
```bash
mkdir -p ~/.local/bin
ln -sf /usr/bin/batcat ~/.local/bin/bat
```

### 2. "command not found: fd"

**Cause:** apt package is named `fd-find`, command is `fdfind`.

**Solution:**
```bash
mkdir -p ~/.local/bin
ln -sf /usr/bin/fdfind ~/.local/bin/fd
```

### 3. "~/.local/bin not in PATH"

**Cause:** PATH not updated after creating symlinks.

**Solution:**
```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### 4. "cargo: command not found"

**Cause:** Rust not installed (needed for sd).

**Solution:**
```bash
tmp_rustup="$(mktemp)"
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$tmp_rustup"
sh "$tmp_rustup" -y
rm -f "$tmp_rustup"
source "$HOME/.cargo/env"
```

### 5. yq permission denied

**Cause:** yq binary not executable.

**Solution:**
```bash
chmod +x ~/.local/bin/yq
```

### 6. eza icons not showing

**Cause:** Terminal doesn't support Nerd Fonts.

**Solution:**
1. Install a Nerd Font: [nerdfonts.com](https://www.nerdfonts.com/)
2. Configure terminal to use it
3. Or use without icons: `eza --no-icons`

---

## Performance Benchmarks

| Tool | vs Classic | Example |
|------|------------|---------|
| fd vs find | ~9x faster | 50k files in 50ms vs 450ms |
| rg vs grep | ~10x faster | 1GB repo in 200ms vs 2s |
| bat vs cat | Similar + features | Syntax highlighting adds ~10ms |
| sd vs sed | ~2x faster | Regex replacement |

---

## Integration with Fish Shell

Add to `~/.config/fish/config.fish`:

```fish
# Aliases for Layer 1 tools
abbr -a ls 'eza'
abbr -a ll 'eza -la'
abbr -a lt 'eza --tree --level=2'
abbr -a cat 'bat'
abbr -a find 'fd'
abbr -a grep 'rg'
abbr -a sed 'sd'
```

---

## Verification

```bash
# Check all tools
for tool in bat fd rg sd jq yq eza; do
    if command -v $tool &>/dev/null; then
        echo "✅ $tool: $($tool --version 2>/dev/null | head -1)"
    else
        echo "❌ $tool: NOT INSTALLED"
    fi
done
```

---

## Next Steps

After Layer 1 is complete:
- **Layer 2: Productivity** - fzf, zoxide, atuin, uv, bun, watchexec, glow, bottom, hyperfine

```bash
./scripts/linux/install-layer-2.sh
```
