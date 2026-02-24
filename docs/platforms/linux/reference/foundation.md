# Foundation Layer - Complete Guide

> Terminal + Shell + Prompt configuration for optimized development

## Overview

The Foundation layer provides the core terminal environment:

| Component | Score | Purpose |
|-----------|-------|---------|
| **rldyourterm** | 91.1 | GPU-accelerated terminal emulator |
| **Fish** | 94.5 | Modern shell with autosuggestions |
| **Starship** | 80.8 | Minimal cross-shell prompt |

**Performance target**: ~30ms shell startup, <5ms input latency

Platform-specific production runbooks:

- Linux: `docs/platforms/linux/README.md`
- macOS: `docs/platforms/macos/README.md`
- Windows: `docs/platforms/windows/README.md`

---

## Installation

### 1. rldyourterm

```bash
# Add rldyourterm repository
tmp_key="$(mktemp)"
curl --proto '=https' --tlsv1.2 -fsSL https://apt.fury.io/rldyourterm/gpg.key -o "$tmp_key"
# Keep the upstream terminal package repository source; keyring filename is
# namespaced for this installer.
sudo gpg --yes --dearmor -o /usr/share/keyrings/rldyourterm-archive-keyring.gpg "$tmp_key"
rm -f "$tmp_key"

# Add to apt sources
echo "deb [signed-by=/usr/share/keyrings/rldyourterm-archive-keyring.gpg] https://apt.fury.io/rldyourterm/ * *" | sudo tee /etc/apt/sources.list.d/rldyourterm.list

# Install
# Keep package namespace aligned with this repo.
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends rldyourterm
```

**Verify:**
```bash
rldyourterm-stable --version
# Expected: rldyourterm 1.0.0 or newer
```

### 2. Fish Shell

```bash
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends fish
```

**Verify:**
```bash
fish --version
# Expected: fish, version 4.5.0 or newer
```

**Set as default shell (optional):**
```bash
chsh -s $(which fish)
```

### 3. Starship Prompt

```bash
tmp_starship="$(mktemp)"
curl --proto '=https' --tlsv1.2 -fsSL https://starship.rs/install.sh -o "$tmp_starship"
sh "$tmp_starship" -y
rm -f "$tmp_starship"
```

**Verify:**
```bash
starship --version
# Expected: starship 1.24.2 or newer
```

---

## Configuration

### Config File Locations

| Component | Config File |
|-----------|-------------|
| rldyourterm | `~/.rldyourterm.lua` |
| Fish | `~/.config/fish/config.fish` |
| Starship | `~/.config/starship.toml` |

### Apply Configurations

From project directory:
```bash
cp configs/rldyourterm/rldyourterm.lua ~/.rldyourterm.lua
cp configs/fish/config.fish ~/.config/fish/config.fish
cp configs/starship/starship.toml ~/.config/starship.toml
```

---

## rldyourterm Configuration

### Philosophy

**Priority: Stability > Speed > Features**

The config is optimized for long-running AI sessions. If rldyourterm GUI crashes, the multiplexer ensures your session survives.

### Key Features

- **OpenGL rendering** - Stable GPU acceleration (WebGPU+Vulkan can cause SIGSEGV with NVIDIA 580.x)
- **Stability-first display protocol** - Linux baseline uses X11/XWayland; Wayland is explicit opt-in
- **Built-in multiplexer** - Session persistence, no tmux needed
- **local_echo_threshold_ms=10** - Predictive echo for instant feel
- **Disabled ligatures** - Faster text rendering for AI output streaming
- **Stable resize profile** - `RLDYOURTERM_STABLE_RESIZE=1` keeps full UI and reduces resize repaint pressure
- **Optional minimal UI mode** - `RLDYOURTERM_MINIMAL_UI=1` for compositor stress scenarios

### Keybindings

| Shortcut | Action |
|----------|--------|
| Ctrl+Shift+\| | Split horizontal |
| Ctrl+Shift+_ | Split vertical |
| Ctrl+Shift+w | Close pane |
| Ctrl+Shift+h/j/k/l | Navigate panes |
| Ctrl+Shift+H/J/K/L | Resize panes |
| Ctrl+Shift+t | New tab |
| Ctrl+Tab | Next tab |
| Ctrl+Shift+Tab | Previous tab |
| Ctrl+Shift+f | Search |
| Ctrl+Shift+c | Copy |
| Ctrl+Shift+v | Paste |
| Ctrl+Shift+r | Reload config |
| Ctrl+Shift+L | Debug overlay |
| Ctrl+Shift+P | Command palette |
| Ctrl+Shift+E | Toggle ligatures |

### Quick Select Patterns

Patterns optimized for AI tool output:

```lua
config.quick_select_patterns = {
  '/[a-zA-Z0-9_./-]+/[a-zA-Z0-9_.-]+',  -- Absolute paths
  '~/[a-zA-Z0-9_./-]+',                  -- Home paths
  'https?://[^\\s]+',                    -- URLs
  '\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}(:\\d+)?',  -- IP:port
  '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}',  -- UUIDs
}
```

### Hyperlink Rules

Clickable links in AI tool output:

```lua
config.hyperlink_rules = rldterm.default_hyperlink_rules()

-- File paths
table.insert(config.hyperlink_rules, {
  regex = '/[a-zA-Z0-9_./-]+/[a-zA-Z0-9_.-]+',
  format = 'file://$0',
})
```

### GPU Configuration

```lua
-- OpenGL is default stable renderer
config.front_end = 'OpenGL'  -- or RLDYOURTERM_RENDERER=opengl

-- Runtime overrides:
-- RLDYOURTERM_RENDERER=software
-- RLDYOURTERM_RENDERER=webgpu
-- RLDYOURTERM_SAFE_RENDERER=1  -- equivalent software fallback
```

### Display Protocol Selection

```lua
-- Linux default (stability-first):
-- enable_wayland=false  -> X11/XWayland baseline

-- Force overrides:
-- RLDYOURTERM_FORCE_X11=1
-- RLDYOURTERM_FORCE_WAYLAND=1
-- RLDYOURTERM_AUTO_WAYLAND=1  -- session-aware auto mode
```

### Multiplexer Configuration

```lua
config.unix_domains = {
  {
    name = 'unix',
    local_echo_threshold_ms = 10,
  },
}

config.default_gui_startup_args = { 'connect', 'unix' }
```

### Appearance Configuration

Default profile keeps fancy tab bar and gradient. Minimal profile disables both:

```lua
config.use_fancy_tab_bar = true
-- RLDYOURTERM_STABLE_RESIZE=1 -> lower status repaint cadence, simpler gradient
-- RLDYOURTERM_MINIMAL_UI=1 -> use_fancy_tab_bar=false, gradient disabled
config.window_decorations = 'TITLE | RESIZE'  -- Classic title bar
config.hide_tab_bar_if_only_one_tab = false
config.show_new_tab_button_in_tab_bar = true
```

### Text Rendering Optimization

```lua
-- Disable ligatures for faster text rendering
-- AI tools stream code with symbols (=>, !=, ===)
config.harfbuzz_features = { 'calt=0', 'clig=0', 'liga=0' }

-- Toggle ligatures with Ctrl+Shift+E
```

---

## Fish Shell Configuration

### Key Features

- **Conditional sourcing** - Tools only initialized if installed (`if type -q`)
- **fish_add_path** - Proper PATH persistence without duplication
- **~30ms startup** - Benchmark with `fish --profile-startup /tmp/fish.prof -ic exit`
- **Transient prompt ready** - Cleaner scrollback (optional)

### Initialization Order

Tools are initialized with conditional checks for stability:

```fish
# Only source if tool is installed
if type -q starship
    starship init fish | source
end

if type -q zoxide
    zoxide init fish | source
end

if type -q atuin
    atuin init fish --disable-up-arrow | source
end

if type -q fzf
    fzf --fish | source
end
```

**Why conditional sourcing?**
- No errors if tools not installed yet
- Works immediately after fresh install
- Graceful degradation

### PATH Management

Use `fish_add_path` for persistence (added to universal variables):

```fish
fish_add_path ~/.local/bin        # AI tools
fish_add_path ~/.cargo/bin        # Rust tools
fish_add_path ~/.atuin/bin        # Atuin
```

### Key Abbreviations

**Navigation:**
| Abbr | Command |
|------|---------|
| z | zoxide |
| zz | z - (previous directory) |
| .. | cd .. |
| ... | cd ../.. |

**File Operations:**
| Abbr | Command |
|------|---------|
| ls | eza |
| ll | eza -la |
| lt | eza --tree --level=2 |
| cat | bat |

**Git:**
| Abbr | Command |
|------|---------|
| g | git |
| gs | git status |
| ga | git add |
| gc | git commit |
| gp | git push |
| gl | git log --oneline -10 |
| gd | git diff |
| lg | lazygit |

**GitHub CLI:**
| Abbr | Command |
|------|---------|
| gh | gh |
| ghp | gh pr |
| ghi | gh issue |
| ghr | gh repo |

**AI Tools (Layer 5):**
| Abbr | Command |
|------|---------|
| cl | claude |
| gem | gemini |
| cx | codex |

### Custom Functions

```fish
# Quick project jump
function proj
    cd ~/projects/$argv[1]
end

# Create directory and enter
function mkcd
    mkdir -p $argv[1] && cd $argv[1]
end

# Quick backup
function backup
    cp $argv[1] $argv[1].bak.(date +%Y%m%d_%H%M%S)
end
```

---

## Starship Configuration

### Format

```toml
format = "$directory$git_branch$git_status$character"
right_format = "$cmd_duration$jobs"
```

### CPU Optimization

```lua
config.cursor_blink_ease_in = 'Constant'
config.cursor_blink_ease_out = 'Constant'
config.text_blink_rate = 0
config.text_blink_rate_rapid = 0
config.check_for_updates = false
config.audible_bell = 'Disabled'
config.visual_bell = { fade_in_duration_ms = 0, fade_out_duration_ms = 0 }
config.window_close_confirmation = 'NeverPrompt'
config.skip_close_confirmation_for_processes_named = {
  'bash', 'sh', 'zsh', 'fish', 'tmux', 'nu', 'cmd.exe', 'pwsh.exe',
}

-- Instant Alt key response
config.send_composed_key_when_left_alt_is_pressed = false
config.send_composed_key_when_right_alt_is_pressed = false
```

| Setting | Value | Effect |
|---------|-------|--------|
| cursor_blink_ease | Constant | No easing calculations |
| text_blink_rate | 0 | No text blinking |
| check_for_updates | false | No background network |
| audible_bell | Disabled | No sound processing |
| visual_bell | 0ms | No animation |
| window_close_confirmation | NeverPrompt | Instant close |

### Performance Settings

### Disabled Modules

40+ modules explicitly disabled for performance:
- package, nodejs, python, rust, golang
- docker_context, kubernetes, aws, gcloud
- azure, bun, c, java, helm, ruby, scala

---

## Troubleshooting

### Common Errors and Solutions

#### 1. rldyourterm: "CurrentDomain is not a valid Pattern variant"

**Error:**
```
Configuration Error: `CurrentDomain` is not a valid Pattern variant
```

**Cause:** Old/incorrect Search action syntax in config.

**Solution:**
Replace:
```lua
{ key = 'f', mods = 'CTRL|SHIFT', action = act.Search 'CurrentDomain' },
```

With:
```lua
{ key = 'f', mods = 'CTRL|SHIFT', action = act.Search 'CurrentSelectionOrEmptyString' },
```

Valid Search patterns:
- `CurrentSelectionOrEmptyString` - Search using selected text
- `CaseSensitiveString = 'text'` - Case-sensitive search
- `CaseInSensitiveString = 'text'` - Case-insensitive search
- `Regex = '[a-f0-9]{6,}'` - Regex search

#### 2. Fish: "Unknown command: zoxide"

**Error:**
```
fish: Unknown command: zoxide
```

**Cause:** zoxide not installed yet (Layer 2 tool).

**Solution:**
1. Install zoxide (Layer 2): `cargo install --locked zoxide`
2. Or comment out the line in config.fish temporarily

#### 3. Fish: "Unknown command: atuin"

**Error:**
```
fish: Unknown command: atuin
```

**Cause:** atuin not installed yet (Layer 2 tool).

**Solution:**
1. Install atuin (Layer 2) safely:
   ```bash
   tmp_atuin="$(mktemp)"
   curl --proto '=https' --tlsv1.2 -fsSL https://setup.atuin.sh -o "$tmp_atuin"
   sh "$tmp_atuin"
   rm -f "$tmp_atuin"
   ```
2. Or comment out the line in config.fish temporarily

#### 4. Starship: "source: not enough arguments"

**Error:**
```
source: not enough arguments
```

**Cause:** Running `starship init fish | source` in wrong shell (zsh/bash instead of fish).

**Solution:**
Switch to fish first:
```bash
fish
```
Then run:
```fish
starship init fish | source
```

Or add to correct config file:
- bash: `~/.bashrc` → `eval "$(starship init bash)"`
- zsh: `~/.zshrc` → `eval "$(starship init zsh)"`
- fish: `~/.config/fish/config.fish` → `starship init fish | source`

#### 5. rldyourterm: GPG key import fails

**Error:**
```
gpg: missing argument for option "-o"
```

**Cause:** Wrong path in command (e.g., `/usr/shake/` instead of `/usr/share/`).

**Solution:**
Use correct path:
```bash
curl -fsSL https://apt.fury.io/rldyourterm/gpg.key | sudo gpg --yes --dearmor -o /usr/share/keyrings/rldyourterm-archive-keyring.gpg
```

Note: `/usr/share/` not `/usr/shake/`

#### 6. rldyourterm: "Permission denied" for apt sources

**Error:**
```
tee: /etc/apt/sources.list.d/rldyourterm.list: Permission denied
```

**Solution:**
Use sudo:
```bash
echo "deb [signed-by=/usr/share/keyrings/rldyourterm-archive-keyring.gpg] https://apt.fury.io/rldyourterm/ * *" | sudo tee /etc/apt/sources.list.d/rldyourterm.list
```

#### 7. Fish: Config not loading

**Cause:** Config file not in correct location.

**Solution:**
```bash
mkdir -p ~/.config/fish
cp configs/fish/config.fish ~/.config/fish/config.fish
```

Verify:
```bash
ls -la ~/.config/fish/config.fish
```

#### 8. Starship: Prompt not showing

**Cause:** Starship not initialized in shell config.

**Solution:**
Add to shell config:
```bash
# For fish
echo 'starship init fish | source' >> ~/.config/fish/config.fish

# For bash
echo 'eval "$(starship init bash)"' >> ~/.bashrc

# For zsh
echo 'eval "$(starship init zsh)"' >> ~/.zshrc
```

Then reload:
```bash
source ~/.config/fish/config.fish  # fish
source ~/.bashrc                    # bash
source ~/.zshrc                     # zsh
```

#### 9. Starship: "TOML parse error ... duplicate key"

**Error:**
```
[ERROR] - (starship::config): Unable to parse the config file: TOML parse error at line 154, column 2
    |
154 | [jobs]
    |  ^^^^
duplicate key
```

**Cause:** Duplicate section headers in starship.toml (e.g., two `[jobs]` or `[gcloud]` sections).

**Solution:**
1. Find duplicates:
```bash
awk '/^\[/{if(seen[$0]++)print NR": "$0}' ~/.config/starship.toml
```

2. Remove the duplicate section manually or re-download the config:
```bash
cp /path/to/project/configs/starship/starship.toml ~/.config/starship.toml
```

3. Verify:
```bash
starship --version
```

---

## Verification

### Check All Components

```bash
# rldyourterm
rldyourterm-stable --version

# Fish
fish --version

# Starship
starship --version

# Config files
ls -la ~/.rldyourterm.lua
ls -la ~/.config/fish/config.fish
ls -la ~/.config/starship.toml
```

### Test Fish Startup Time

```bash
time fish -c exit
# Expected: < 0.05s total
```

### Test rldyourterm Config

```bash
rldyourterm-stable -e echo "test"
# Should open terminal without errors
```

---

## Shell Integration Reference

| Shell | Init Command | Config File |
|-------|--------------|-------------|
| bash | `eval "$(starship init bash)"` | `~/.bashrc` |
| zsh | `eval "$(starship init zsh)"` | `~/.zshrc` |
| fish | `starship init fish \| source` | `~/.config/fish/config.fish` |

---

## Next Steps

After Foundation is complete:

1. **Layer 1: File Operations** - bat, fd, rg, sd, jq, yq, eza
2. **Layer 2: Productivity** - fzf, zoxide, atuin, uv, bun
3. **Layer 3: GitHub & Git** - gh CLI, lazygit, delta
4. **Layer 4: Code Intelligence** - grepai, ast-grep, probe, semgrep

---

## Related Files

- `configs/rldyourterm/rldyourterm.lua` - rldyourterm configuration
- `configs/fish/config.fish` - Fish shell configuration
- `configs/starship/starship.toml` - Starship prompt configuration
- `.serena/memories/FRONTEND_00_terminal.md` - Detailed terminal config memory
