# Windows

Production-ready Windows implementation using PowerShell + WinGet with layered
install scripts and health-check parity.

## Scope

- OS: Windows 10/11
- Package manager: WinGet (primary)
- Shell baseline: PowerShell (`pwsh`)
- Terminal: rldyourterm
- Prompt: Starship
- Layers: Foundation + Layer 1..5

## Entrypoints

From PowerShell (recommended):

```powershell
.\scripts\install-windows.ps1
.\scripts\health-check-windows.ps1 -Summary
.\scripts\install-windows.ps1 -Help
.\scripts\health-check-windows.ps1 -Help
```

Flow smoke check (no installs):

```powershell
.\scripts\install-windows.ps1 -DryRun
```

From Git Bash / MSYS2 / Cygwin:

```bash
./scripts/install.sh
./scripts/health-check.sh --summary
```

`install.sh` and `health-check.sh` auto-detect Windows-like shells and delegate
into PowerShell scripts.

## Windows Layer Scripts

- `scripts/windows/install.ps1`
- `scripts/windows/install-foundation.ps1`
- `scripts/windows/install-layer-1.ps1`
- `scripts/windows/install-layer-2.ps1`
- `scripts/windows/install-layer-3.ps1`
- `scripts/windows/install-layer-4.ps1`
- `scripts/windows/install-layer-5.ps1`
- `scripts/windows/health-check.ps1`

## Tooling Model by Layer

### Foundation

- WinGet: `rldyourterm` (mapped to command `rldyourterm` in this distribution), `Microsoft.PowerShell`, `Starship.Starship`, `Git.Git`
- Optional font: `DEVCOM.JetBrainsMonoNerdFont`
- Config sync:
  - `configs/rldyourterm/rldyourterm.lua` -> `~/.rldyourterm.lua`
  - `configs/starship/starship.toml` -> `~/.config/starship.toml`
- Profile bootstrapping:
  - `Invoke-Expression (&starship init powershell)`
  - `Invoke-Expression (& { (zoxide init powershell | Out-String) })`
- WinGet install scope strategy:
  - tries `--scope user`
  - if needed retries with `--scope machine`

### Layer 1 (File Operations)

- WinGet:
  - `sharkdp.bat`
  - `sharkdp.fd`
  - `BurntSushi.ripgrep.MSVC`
  - `chmln.sd`
  - `jqlang.jq`
  - `MikeFarah.yq`
  - `eza-community.eza`

### Layer 2 (Productivity)

- WinGet:
  - `junegunn.fzf`
  - `ajeetdsouza.zoxide`
  - `Atuinsh.Atuin`
  - `astral-sh.uv` (fallback: official `astral.sh` PowerShell installer)
  - `Oven-sh.Bun` (fallback: official `bun.sh` PowerShell installer)
  - `charmbracelet.glow`
  - `Clement.bottom`
  - `sharkdp.hyperfine`
- Cargo fallback:
  - `watchexec-cli` -> `watchexec`

### Layer 3 (GitHub & Git)

- WinGet:
  - `GitHub.cli`
  - `JesseDuffield.lazygit`
  - `dandavison.delta`
- Git defaults:
  - delta pager integration
  - aliases (`co`, `br`, `ci`, `st`, `visual`, `lg`, ...)

### Layer 4 (Code Intelligence)

- WinGet:
  - `UniversalCtags.Ctags`
  - `XAMPPRocky.Tokei`
  - `ast-grep.ast-grep`
- Cargo:
  - `probe-code` -> `probe`
- uv tool install:
  - `semgrep`
- GitHub release fallback:
  - `grepai` Windows zip asset -> `~/.local/bin/grepai.exe`

### Layer 5 (AI Orchestration)

- WinGet:
  - `OpenJS.NodeJS.LTS`
- npm globals:
  - `@anthropic-ai/claude-code`
  - `@google/gemini-cli`
  - `@openai/codex`

## Fish on Windows

Native Windows flow uses PowerShell + Starship. Fish integration is recommended
via WSL distribution (install fish inside WSL), because Fish support on Windows
is primarily through compatibility environments (WSL/MSYS2/Cygwin).

Shared rldyourterm config is platform-gated with `rldterm.target_triple` (API module variable in this terminal project):

- Windows defaults to `pwsh`/`powershell.exe`
- macOS/Linux defaults to Fish when available

## Health Check

```powershell
.\scripts\health-check-windows.ps1 -Summary
.\scripts\health-check-windows.ps1 -Strict
```

## CI Coverage

- PowerShell parse validation for all `*.ps1` scripts
- Windows flow smoke: `.\scripts\install-windows.ps1 -DryRun`

Health check validates:

- PowerShell script parse integrity (`*.ps1`)
- Tool presence/version across all layers
- rldyourterm and Starship config parity
- PowerShell profile init lines for Starship/zoxide
- PATH requirements (`~/.local/bin`)

## Sources Used for Implementation

- WinGet CLI docs and non-interactive flags
- WinGet manifests in `microsoft/winget-pkgs`
- PowerShell profile and execution policy docs
- rldyourterm Windows target-triple/default-program docs
- Starship PowerShell init docs
- uv Windows installer docs
- Bun Windows installer docs
- Semgrep CLI install docs
