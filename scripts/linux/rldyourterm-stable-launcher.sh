#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
# rldyourterm Stable Launcher
# ═══════════════════════════════════════════════════════════════════════════════
# Use stable defaults for heavy multi-terminal AI workflows.
#
# Supported launch profiles:
# - stable   (default): X11/XWayland + stable resize profile
# - minimal: X11/XWayland + minimal UI
# - wayland: native Wayland
# - software: software renderer fallback
# - passthrough: no overrides

set -euo pipefail

SCRIPT_NAME="rldyourterm-stable"

clear_rldyourterm_mode_env() {
  unset RLDYOURTERM_FORCE_X11
  unset RLDYOURTERM_FORCE_WAYLAND
  unset RLDYOURTERM_AUTO_WAYLAND
  unset RLDYOURTERM_STABLE_RESIZE
  unset RLDYOURTERM_MINIMAL_UI
  unset RLDYOURTERM_SAFE_RENDERER
  unset RLDYOURTERM_RENDERER
}

warn_unexpected_args() {
  local -a unexpected_args=("$@")

  printf '%s: ignoring positional arguments when launching rldyourterm window: %s\n' \
    "$SCRIPT_NAME" \
    "${unexpected_args[*]}" >&2
  printf 'Use %s -- -- <command> [args] or export RLDYOURTERM_STABLE_ALLOW_COMMAND_ARGS=1\n' \
    "$SCRIPT_NAME" >&2

  return 2
}

disable_compositor_extensions() {
  if ! command -v gnome-extensions >/dev/null 2>&1; then
    return
  fi

  if ! command -v gsettings >/dev/null 2>&1; then
    return
  fi

  if ! rg -q 'ubuntu:GNOME|GNOME' <<<"${XDG_CURRENT_DESKTOP:-}"; then
    return
  fi

  if [[ -n "${RLDYOURTERM_SAFE_EXTENSIONS:-}" && "${RLDYOURTERM_SAFE_EXTENSIONS}" != "1" ]]; then
    return
  fi

  # These extensions are known to emit frequent MetaShapedTexture/handle-move warnings
  # on aggressive window moves/resizes with many terminals.
  local -a gnome_targets=(
    'rounded-window-corners@fxgn'
    'tiling-assistant@ubuntu.com'
  )

  local -a enabled_exts
  local ext

  IFS=$'\n' read -r -d '' -a enabled_exts < <(gnome-extensions list --enabled 2>/dev/null && printf '\0')

  COMPOSITOR_EXTENSIONS_TO_RESTORE=()
  for ext in "${gnome_targets[@]}"; do
    if printf '%s\n' "${enabled_exts[@]}" | rg -qx "$ext"; then
      if gnome-extensions disable "$ext" >/dev/null 2>&1; then
        COMPOSITOR_EXTENSIONS_TO_RESTORE+=("$ext")
      fi
    fi
  done
}

restore_compositor_extensions() {
  local ext
  for ext in "${COMPOSITOR_EXTENSIONS_TO_RESTORE[@]-}"; do
    gnome-extensions enable "$ext" >/dev/null 2>&1 || true
  done
}

trap restore_compositor_extensions EXIT INT TERM

print_usage() {
  cat <<'USAGE'
Usage: rldyourterm-stable [--mode stable|minimal|wayland|software|passthrough] [--] [rldyourterm args]

  --help, -h        Show this help.
  --mode MODE       Select launch profile (default: stable).
  --command          Spawn the arguments after this flag as the command to run.
                    This is equivalent to using '--' and bypasses path handling.
  --allow-raw-args   Accept raw positional arguments as command arguments.
                    Set with caution; this can trigger `Unable to spawn <arg>` errors.
Examples:
  rldyourterm-stable --mode stable
  rldyourterm-stable --mode minimal
  rldyourterm-stable --mode wayland
  rldyourterm-stable --mode software
  rldyourterm-stable --mode passthrough
  rldyourterm-stable -- -- vim
  rldyourterm-stable --command bash -lc 'cd /tmp && exec fish'
  RLDYOURTERM_STABLE_ALLOW_COMMAND_ARGS=1 rldyourterm-stable 'some-command'
USAGE
}

resolve_upstream_terminal_binary() {
  local candidate
  local resolved

  # Prioritize distro-installed binaries, then fallback to PATH search.
  for candidate in \
    /usr/bin/rldyourterm \
    /usr/local/bin/rldyourterm \
    /usr/bin/rldyourterm-gui \
    /usr/local/bin/rldyourterm-gui \
    "$(command -v rldyourterm || true)" \
    "$(command -v rldyourterm-gui || true)"; do
    [[ -z "$candidate" ]] && continue
    [[ ! -x "$candidate" ]] && continue

    resolved="$(realpath "$candidate" 2>/dev/null || true)"
    if [[ -z "$resolved" ]]; then
      continue
    fi

    if [[ "$resolved" != "$SCRIPT_PATH" ]]; then
      printf '%s' "$candidate"
      return 0
    fi
  done

  return 1
}

SCRIPT_PATH="$(realpath "${BASH_SOURCE[0]}")"
RLDYOURTERM_UPSTREAM="$(resolve_upstream_terminal_binary || true)"

if [[ -z "${RLDYOURTERM_UPSTREAM:-}" ]] || [[ ! -x "$RLDYOURTERM_UPSTREAM" ]]; then
    printf "%s requires rldyourterm terminal backend in PATH\n" "$SCRIPT_NAME" >&2
    exit 1
fi

if [[ "${RLDYOURTERM_UPSTREAM}" == "${SCRIPT_PATH}" ]]; then
    printf "%s resolved backend to launcher itself; refusing recursion\n" "$SCRIPT_NAME" >&2
    exit 1
fi

if [[ $# -gt 0 ]]; then
  case "$1" in
    --version|-V|help|ssh|serial|connect|cli|imgcat|set-working-directory|record|replay|shell-completion)
      exec "$RLDYOURTERM_UPSTREAM" "$@"
      ;;
  esac
fi

MODE="stable"
ALLOW_RAW_ARGS="${RLDYOURTERM_STABLE_ALLOW_COMMAND_ARGS:-0}"
STARTUP_ARGS=(--always-new-process)
RLDYOURTERM_CMD_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      print_usage
      exit 0
      ;;
    --mode)
      if [[ $# -lt 2 ]]; then
        printf '%s\n' "--mode requires a value" >&2
        exit 1
      fi
      MODE="$2"
      shift 2
      ;;
    --mode=*)
      MODE="${1#*=}"
      shift
      ;;
    --command)
      shift
      if [[ $# -eq 0 ]]; then
        printf '%s\n' "--command requires at least one argument" >&2
        exit 1
      fi
      RLDYOURTERM_CMD_ARGS=("$@")
      shift "$#"
      break
      ;;
    start)
      shift
      if [[ $# -gt 0 ]]; then
        RLDYOURTERM_CMD_ARGS=("$@")
      fi
      break
      ;;
    --)
      shift
      RLDYOURTERM_CMD_ARGS=("$@")
      shift "$#"
      break
      ;;
    -*)
      break
      ;;
      *)
      break
      ;;
  esac
done

if [[ "${#RLDYOURTERM_CMD_ARGS[@]}" -eq 0 && "$#" -gt 0 ]]; then
  if [[ "$1" == "start" ]]; then
    shift
    if [[ "$#" -gt 0 ]]; then
      RLDYOURTERM_CMD_ARGS=("$@")
    fi
  elif [[ "$ALLOW_RAW_ARGS" == "1" || "$ALLOW_RAW_ARGS" == "true" ]]; then
    RLDYOURTERM_CMD_ARGS=("$@")
  elif [[ "$#" -eq 1 && -d "$1" ]]; then
    STARTUP_ARGS+=(--cwd "$1")
  else
    warn_unexpected_args "$@"
    exit 2
  fi
fi

case "$MODE" in
  stable)
    clear_rldyourterm_mode_env
    export RLDYOURTERM_FORCE_X11=1
    export RLDYOURTERM_STABLE_RESIZE=1
    export RLDYOURTERM_RENDERER=opengl
    export RLDYOURTERM_SAFE_EXTENSIONS=1
    unset RLDYOURTERM_MINIMAL_UI
    disable_compositor_extensions
    ;;
  minimal)
    clear_rldyourterm_mode_env
    export RLDYOURTERM_FORCE_X11=1
    export RLDYOURTERM_STABLE_RESIZE=1
    export RLDYOURTERM_MINIMAL_UI=1
    export RLDYOURTERM_RENDERER=opengl
    export RLDYOURTERM_SAFE_EXTENSIONS=1
    disable_compositor_extensions
    ;;
  wayland)
    clear_rldyourterm_mode_env
    export RLDYOURTERM_FORCE_WAYLAND=1
    export RLDYOURTERM_RENDERER=opengl
    ;;
  x11)
    clear_rldyourterm_mode_env
    export RLDYOURTERM_FORCE_X11=1
    export RLDYOURTERM_RENDERER=opengl
    ;;
  software)
    clear_rldyourterm_mode_env
    export RLDYOURTERM_FORCE_X11=1
    export RLDYOURTERM_STABLE_RESIZE=1
    export RLDYOURTERM_SAFE_RENDERER=1
    export RLDYOURTERM_RENDERER=software
    ;;
  passthrough)
    clear_rldyourterm_mode_env
    ;;
  *)
    printf "Unknown mode: %s\n" "$MODE" >&2
    print_usage
    exit 1
    ;;
esac

if [[ "${#RLDYOURTERM_CMD_ARGS[@]}" -gt 0 ]]; then
  STARTUP_ARGS+=("${RLDYOURTERM_CMD_ARGS[@]}")
fi

exec "$RLDYOURTERM_UPSTREAM" start "${STARTUP_ARGS[@]}"
