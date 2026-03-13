#!/usr/bin/env bash

require_display_session() {
  case "$(uname -s)" in
    Linux)
      if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
        echo "live display benchmark requires DISPLAY or WAYLAND_DISPLAY on Linux" >&2
        exit 2
      fi
      ;;
    Darwin) ;;
    *) ;;
  esac
}
