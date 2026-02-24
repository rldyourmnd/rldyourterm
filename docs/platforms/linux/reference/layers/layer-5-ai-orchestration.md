# Layer 5: AI Orchestration

> Multi-model CLI tools for terminal workflows

## Overview

Layer 5 installs and uses three AI CLIs:

- Claude Code (`@anthropic-ai/claude-code`)
- Gemini CLI (`@google/gemini-cli`)
- Codex CLI (`@openai/codex`)

This layer is cross-platform and is executed by:

- Linux: `scripts/linux/install-layer-5.sh`
- macOS: `scripts/macos/install-layer-5.sh`
- Windows: `scripts/windows/install-layer-5.ps1`

## Installation

### Scripted

```bash
./scripts/linux/install-layer-5.sh
```

### Manual

```bash
# Node.js is required for all tools
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends nodejs npm

# Avoid EACCES issues for global npm installs
npm config set prefix "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"

npm install -g --no-fund --no-audit --loglevel=error @anthropic-ai/claude-code
npm install -g --no-fund --no-audit --loglevel=error @google/gemini-cli
npm install -g --no-fund --no-audit --loglevel=error @openai/codex
```

## Authentication

### Claude Code

```bash
claude
```

### Gemini CLI

```bash
# OAuth
gemini

# API key mode
export GEMINI_API_KEY="your-key"
gemini
```

### Codex CLI

```bash
export OPENAI_API_KEY="your-key"
codex
```

## Basic Usage

```bash
claude "Explain this repository"
gemini -p "Research best practices for this stack"
codex exec --sandbox read-only "Review this code for regressions"
```

## Verification

```bash
claude --version
gemini --version
codex --version
node --version
npm --version
```
