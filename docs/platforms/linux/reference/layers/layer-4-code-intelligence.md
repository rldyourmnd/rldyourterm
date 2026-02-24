# Layer 4: Code Intelligence - Complete Guide

> Advanced code analysis and search tools

## Overview

This layer provides semantic code search, AST-aware editing, and security analysis. These tools enable deep understanding of codebases.

## Tools

| Tool | Score | Purpose |
|------|-------|---------|
| **grepai** | 88.4 | Semantic code search with embeddings |
| **ast-grep** | 78.7 | AST structural search and rewrite |
| **probe** | N/A | Code block extraction |
| **semgrep** | 70.4 | Static analysis for security |
| **ctags** | N/A | Code indexing for navigation |
| **tokei** | N/A | Code statistics by language |

## Installation

### Quick Install

```bash
./scripts/linux/install-layer-4.sh
```

### Manual Installation

```bash
# ctags - code indexing (requires sudo)
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends universal-ctags

# tokei - code statistics
cargo install --locked tokei

# ast-grep (78.7) - AST structural search
cargo install --locked ast-grep

# probe - code extraction
cargo install --locked probe-code

# semgrep (70.4) - security analysis
python3 -m pip install --user --break-system-packages semgrep || python3 -m pip install --user semgrep

# grepai (88.4) - semantic code search (Go binary)
GREPAI_VERSION=$(curl -s https://api.github.com/repos/yoanbernabeu/grepai/releases/latest | grep -oP '"tag_name": "\K[^"]+')
case "$(uname -m)" in
  x86_64|amd64) GREPAI_ARCH="x86_64" ;;
  aarch64|arm64|armv8*|armv7*) GREPAI_ARCH="arm64" ;;
  *) GREPAI_ARCH="x86_64" ;;
esac
GREPAI_URL="https://github.com/yoanbernabeu/grepai/releases/download/${GREPAI_VERSION}/grepai_${GREPAI_VERSION#v}_linux_${GREPAI_ARCH}.tar.gz"
tmp_grepai="$(mktemp -d)"
curl --proto '=https' --tlsv1.2 -fSsL -o "$tmp_grepai/grepai.tar.gz" "$GREPAI_URL"
tar -xzf "$tmp_grepai/grepai.tar.gz" -C "$tmp_grepai"
mkdir -p ~/.local/bin
mv "$tmp_grepai/grepai" ~/.local/bin/grepai
chmod +x ~/.local/bin/grepai
rm -rf "$tmp_grepai"
```

## grepai - Semantic Code Search

`★ Key Feature: Natural language queries → finds code by meaning, not just text`

### Usage

```bash
# Initialize (builds embeddings)
grepai init

# Semantic search with natural language
grepai search "user authentication flow"
grepai search "error handling middleware"
grepai search "database connection pool"

# JSON output
grepai search "authentication" --json

# Compact output (saves ~80% tokens)
grepai search "auth flow" --json --compact

# Limit results
grepai search "API" --limit 5

# Search specific workspace
grepai search "login" --workspace myproject
```

### How It Works

1. Indexes code using vector embeddings
2. Builds semantic understanding of code
3. Matches queries by meaning, not just text
4. Perfect for exploring unfamiliar codebases

## ast-grep - Structural Search

`★ Key Feature: Search and rewrite code based on AST patterns`

### Usage

```bash
# Find function definitions
sg -p 'fn $NAME($$$PARAMS) $$$BODY' -l rust

# Find all if statements
sg -p 'if ($COND) { $$$BODY }' -l javascript

# Find Python functions
sg -p 'def $NAME($$$PARAMS): $$$BODY' -l python

# Find Go structs
sg -p 'type $NAME struct { $$$FIELDS }' -l go

# Rewrite code
sg -p 'console.log($MSG)' -r 'logger.info($MSG)' -l typescript

# Interactive mode
sg -p 'var $VAR = $VAL' -l javascript -i
```

### Pattern Syntax

| Syntax | Meaning |
|--------|---------|
| `$NAME` | Single metavariable |
| `$$$BODY` | Multiple statements |
| `$$$` | Zero or more |
| `${NAME}text` | With suffix |

### Supported Languages

- Rust
- JavaScript/TypeScript
- Python
- Go
- Java
- C/C++
- Ruby
- And 20+ more

## probe - Code Extraction

`★ Key Feature: Extracts code blocks optimized for context windows`

### Usage

```bash
# Search with token limit
probe search "prompt injection" ./ --max-tokens 10000

# Query code structures
probe query "fn $NAME($$$PARAMS) $$$BODY" ./src --language rust

# Find Python classes
probe query "class $NAME: $$$BODY" ./src --language python
```

### Features

- Token-aware extraction
- Tree-sitter parsing
- Respects context window limits

## semgrep - Security Analysis

`★ Key Feature: Find security vulnerabilities with pattern matching`

### Usage

```bash
# Scan current directory
semgrep --config auto .

# Scan with specific rules
semgrep --config p/security-audit .

# Custom rule
semgrep --config my-rules.yml .

# Output formats
semgrep --json .
semgrep --sarif .

# CI/CD mode
semgrep --ci
```

## ctags - Code Indexing

### Usage

```bash
# Generate tags for project
ctags -R .

# With language filter
ctags -R --languages=Python,Rust .

# For specific directory
ctags -R src/

# Use with vim
vim -t function_name
```

## tokei - Code Statistics

### Usage

```bash
# Count lines in project
tokei

# By language
tokei --type rust,python

# Sort by lines
tokei --sort lines

# Output as JSON
tokei --output json

# Exclude directories
tokei --exclude node_modules target
```

## Comparison Table

| Feature | grepai | ast-grep | probe | semgrep |
|---------|--------|----------|-------|---------|
| Search type | Semantic | Structural | Hybrid | Pattern |
| Query format | Natural | AST pattern | Both | Rule |
| Token control | Yes | No | Yes | No |
| Security | No | No | No | Yes |
| Rewrite | No | Yes | No | No |

## Developer Workflow

```bash
# 1. Semantic search to understand codebase
grepai search "user registration flow" --json --compact

# 2. Structural search for specific patterns
sg -p 'async def $NAME($$$): $$$BODY' -l python

# 3. Extract code for context
probe search "authentication" ./ --max-tokens 8000

# 4. Security scan
semgrep --config auto .
```
