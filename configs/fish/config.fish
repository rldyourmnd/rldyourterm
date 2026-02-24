# ═══════════════════════════════════════════════════════════════════════════════
# FISH CONFIGURATION - Ultimate AI Terminal Environment
# ═══════════════════════════════════════════════════════════════════════════════
# Priority: Stability > AI CLI compatibility > speed

# ═══════════════════════════════════════════════════════════════════════════════
# PATH SETUP - Deterministic and shell-local (global scope, no universal drift)
# ═══════════════════════════════════════════════════════════════════════════════
set -l __path_candidates \
    ~/.local/bin \
    ~/.claude/local/bin \
    ~/.atuin/bin \
    ~/.cargo/bin \
    ~/.bun/bin \
    ~/.local/share/pnpm \
    /usr/local/go/bin \
    /home/linuxbrew/.linuxbrew/bin \
    /home/linuxbrew/.linuxbrew/sbin \
    ~/.pulumi/bin \
    ~/Android/Sdk/emulator \
    ~/Android/Sdk/platform-tools \
    ~/.local/share/pipx

for path in $__path_candidates
    if test -d $path
        fish_add_path -g $path
    end
end
set -e __path_candidates

# Python venvs (conditional - only add if exists)
if test -d ~/.local/venvs/ai-ml/bin
    fish_add_path -g ~/.local/venvs/ai-ml/bin
end
if test -d ~/.local/venvs/vllm/bin
    fish_add_path -g ~/.local/venvs/vllm/bin
end

# Node (nvm): prefer explicit default alias, normalize "24.13.1" -> "v24.13.1".
if test -d ~/.nvm/versions/node
    set -l nvm_default ""
    if test -f ~/.nvm/alias/default
        set nvm_default (string trim -- (cat ~/.nvm/alias/default))
    end

    if test -n "$nvm_default"; and not string match -rq '^v' -- $nvm_default
        set nvm_default "v$nvm_default"
    end

    if test -n "$nvm_default"; and test -d ~/.nvm/versions/node/$nvm_default/bin
        fish_add_path -g ~/.nvm/versions/node/$nvm_default/bin
    else
        set -l node_version (ls -1 ~/.nvm/versions/node 2>/dev/null | sort -V | tail -1)
        if test -n "$node_version"; and test -d ~/.nvm/versions/node/$node_version/bin
            fish_add_path -g ~/.nvm/versions/node/$node_version/bin
        end
    end
end

# Remove duplicated PATH entries inherited from parent environment (e.g. /snap/bin).
set -l __path_dedup
for path in $PATH
    if not contains -- $path $__path_dedup
        set -a __path_dedup $path
    end
end
set -gx PATH $__path_dedup
set -e __path_dedup

# ═══════════════════════════════════════════════════════════════════════════════
# ENVIRONMENT VARIABLES
# ═══════════════════════════════════════════════════════════════════════════════
set -gx EDITOR nvim
set -gx VISUAL nvim
set -gx PAGER bat

# FZF configuration
set -gx FZF_CTRL_T_OPTS "--walker-skip .git,node_modules,target --preview 'bat -n --color=always {}' --bind 'ctrl-/:change-preview-window(down|hidden|)'"
set -gx FZF_CTRL_R_OPTS "--preview 'echo {}' --preview-window down:3:hidden:wrap --bind '?:toggle-preview'"
set -gx FZF_ALT_C_OPTS "--preview 'eza --tree --level=1 {}'"

set fish_greeting

# Non-interactive fish should only keep deterministic PATH/env.
if not status is-interactive
    return
end

# ═══════════════════════════════════════════════════════════════════════════════
# TOOL INITIALIZATION (interactive)
# ═══════════════════════════════════════════════════════════════════════════════
if command -q starship
    starship init fish | source
end

if command -q zoxide
    zoxide init fish | source
end

if command -q atuin
    atuin init fish --disable-up-arrow | source
end

if command -q fzf
    fzf --fish | source
end

# ═══════════════════════════════════════════════════════════════════════════════
# ABBREVIATIONS
# ═══════════════════════════════════════════════════════════════════════════════
function __set_abbr --argument-names name expansion
    if abbr -q $name
        abbr --erase $name
    end
    abbr --add --global $name $expansion
end

__set_abbr z 'zoxide'
__set_abbr zz 'z -'
__set_abbr .. 'cd ..'
__set_abbr ... 'cd ../..'
__set_abbr .... 'cd ../../..'

__set_abbr ls 'eza'
__set_abbr ll 'eza -la'
__set_abbr lt 'eza --tree --level=2'
__set_abbr cat 'bat'
__set_abbr bottom 'btm'

__set_abbr g 'git'
__set_abbr gs 'git status'
__set_abbr ga 'git add'
__set_abbr gc 'git commit'
__set_abbr gp 'git push'
__set_abbr gl 'git log --oneline -10'
__set_abbr gd 'git diff'
__set_abbr lg 'lazygit'

__set_abbr gh 'gh'
__set_abbr ghp 'gh pr'
__set_abbr ghi 'gh issue'
__set_abbr ghr 'gh repo'

__set_abbr py 'python3'
__set_abbr pip 'uv pip'
__set_abbr venv 'uv venv'

__set_abbr nv 'nvim'
__set_abbr code 'code .'
__set_abbr sp 'starship-profile'
__set_abbr sps 'starship-profile stable-max'
__set_abbr spu 'starship-profile ultra-max'

__set_abbr cl 'claude --dangerously-skip-permissions'
__set_abbr gem 'gemini --yolo'
__set_abbr cx 'codex --full-auto'
__set_abbr wzs 'rldyourterm-stable'
__set_abbr wzt 'rldyourterm-stable --mode stable'
__set_abbr wzm 'rldyourterm-stable --mode minimal'
__set_abbr wzw 'rldyourterm-stable --mode wayland'
__set_abbr wzr 'rldyourterm-stable --mode software'

functions --erase __set_abbr

# ═════════════════════════════════════════════════════════════════════════════
# FUNCTIONS
# ═════════════════════════════════════════════════════════════════════════════
function proj
    if test -n "$argv[1]"
        cd ~/projects/$argv[1]
    end
end

function mkcd
    if test -n "$argv[1]"
        mkdir -p -- "$argv[1]"; and cd -- "$argv[1]"
    end
end

function backup
    if test -f "$argv[1]"
        cp -- "$argv[1]" "$argv[1].bak."(date +%Y%m%d_%H%M%S)
    end
end

function ducks
    du -sh -- * | sort -h
end

function ff
    fd -H $argv
end

function gg
    rg -i $argv
end

function rldyourterm
    if test (count $argv) -eq 0
        command rldyourterm-stable --mode stable
        return
    end

    if test "$argv[1]" = "--"
        command rldyourterm-stable --mode stable -- $argv[2..-1]
        return
    end

    if test "$argv[1]" = "--command"
        if test (count $argv) -gt 1
            command rldyourterm-stable --mode stable --command $argv[2..-1]
        else
            printf 'rldyourterm: --command requires a command to run\n' >&2
            return 2
        end
        return
    end

    if test "$argv[1]" = "start"
        command rldyourterm-stable --mode stable start $argv[2..-1]
        return
    end

    if string match -q -r '^-' -- $argv[1]
        command rldyourterm-stable --mode stable $argv
        return
    end

    printf 'rldyourterm: positional arguments require -- or --command\n' >&2
    printf 'Example: rldyourterm -- -- vim\n' >&2
    return 2
end
