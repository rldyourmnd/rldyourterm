# Terminal & Tools Catalog

This catalog is the Linux (Debian/Ubuntu) source of truth.
For macOS, use `docs/platforms/macos/README.md`.
For Windows, use `docs/platforms/windows/README.md`.
The tables below define Linux layer expectations.

## Terminal Stack

| Component | Layer | Install method | Verification | Rationale |
|---|---|---|---|---|
| rldyourterm | Foundation | `scripts/linux/install-foundation.sh` | `rldyourterm-stable --version` | GPU-accelerated terminal with multiplexer and rich UI primitives. |
| Fish | Foundation | `scripts/linux/install-foundation.sh` | `fish --version` | Modern shell with fast startup and scriptable config model. |
| Starship | Foundation | `scripts/linux/install-foundation.sh` | `starship --version` | Cross-shell, lightweight prompt; prompt profile switching via `starship-profile`. |

## Layer 1 Tools

| Tool | Binary | Install source | Health command | Runtime role |
|---|---|---|---|---|
| bat | `bat` | APT (`bat`) or symlink from `batcat` | `bat --version` | Better file cat with syntax highlighting. |
| fd | `fd` | APT (`fd-find`) with symlink | `fd --version` or `fdfind --version` | Fast `find` replacement. |
| rg | `rg` | APT (`ripgrep`) | `rg --version` | Fast search across repositories. |
| sd | `sd` | `cargo install --locked` | `sd --version` | Streamlined stream editor for safer regex replacements. |
| jq | `jq` | APT (`jq`) | `jq --version` | JSON transformation and extraction. |
| yq | `yq` | Direct download | `yq --version` | YAML/JSON/XML query and edit utility. |
| eza | `eza` | APT (`eza`) | `eza --version` | Modern `ls` replacement with git metadata. |

## Layer 2 Tools

| Tool | Binary | Install source | Health command | Runtime role |
|---|---|---|---|---|
| fzf | `fzf` | Git clone script install | `fzf --version` | Fuzzy search for files/history. |
| zoxide | `zoxide` | `cargo install --locked` | `zoxide --version` | Frecency-based directory jumps. |
| atuin | `atuin` | Installer script (`setup.atuin.sh`) | `atuin --version` | Shared shell history with sync support. |
| uv | `uv` | Installer script | `uv --version` | High-performance Python dependency workflow. |
| bun | `bun` | `bun.sh` installer | `bun --version` | JavaScript runtime for fast JS tooling. |
| watchexec | `watchexec` | `cargo install --locked` | `watchexec --version` | File watcher and command automation loop. |
| glow | `glow` | APT or GitHub tarball fallback | `glow --version` | Terminal markdown rendering in CI notes and docs. |
| btm | `btm` | `cargo install --locked bottom` | `btm --version` | Process and resource monitor. |
| hyperfine | `hyperfine` | `cargo install --locked` | `hyperfine --version` | Benchmarking and reproducibility checks. |

## Layer 3 Tools

| Tool | Binary | Install source | Health command | Runtime role |
|---|---|---|---|---|
| gh | `gh` | APT (`gh`) | `gh --version` | GitHub operations from terminal. |
| lazygit | `lazygit` | GitHub release binary (`amd64`/`arm64`) | `lazygit --version` | Interactive terminal Git TUI. |
| delta | `delta` | `cargo install --locked git-delta` | `delta --version` | Better git diff rendering and side-by-side review. |

## Layer 4 Tools

| Tool | Binary | Install source | Health command | Runtime role |
|---|---|---|---|---|
| grepai | `grepai` | GitHub release binary (`amd64`/`arm64`) | `grepai version` | Semantic search for code understanding. |
| ast-grep | `sg` | `cargo install --locked ast-grep` (binary name can be `sg`) | `sg --version` | AST-aware structural search and rewrite. |
| probe | `probe` | `cargo install --locked probe-code` | `probe --version` | Code extraction for local context windows. |
| semgrep | `semgrep` | `pip install` | `semgrep --version` | Static security and pattern checks. |
| ctags | `ctags` | APT (`universal-ctags`) | `ctags --version` | Tag generation for navigation and editors. |
| tokei | `tokei` | `cargo install --locked` | `tokei --version` | Repo size and language composition. |

## Layer 5 Tools

| Tool | Binary | Install source | Health command | Runtime role |
|---|---|---|---|---|
| Claude CLI | `claude` | `npm install -g @anthropic-ai/claude-code` | `claude --version` | Anthropic model assistant in terminal. |
| Gemini CLI | `gemini` | `npm install -g @google/gemini-cli` | `gemini --version` | Google AI assistant flows. |
| Codex CLI | `codex` | `npm install -g @openai/codex` | `codex --version` | OpenAI CLI integration for coding tasks. |

## Verification Policy

This catalog must be updated when:

- A tool is added, removed, replaced, or renamed.
- Layer boundaries are changed.
- Health checks or runtime behavior diverges from documented values.

After any change, run:

```bash
bash -n scripts/*.sh
fish -n configs/fish/config.fish
./scripts/health-check.sh --summary
```

and update:

- `docs/platforms/linux/README.md` (if behavior changed)
- `CHANGELOG.md` under `[Unreleased]`

The `docs/operations/health-check.md` and `docs/operations/troubleshooting.md` files provide the operational playbook for deviations.
