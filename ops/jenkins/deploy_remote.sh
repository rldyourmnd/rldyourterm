#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 3 ]]; then
  echo "usage: $0 <ssh-target> [remote-root] [remote-env-file]" >&2
  exit 1
fi

ssh_target="$1"
remote_root="${2:-/srv/rldyourterm-jenkins}"
remote_env_file="${3:-$remote_root/.env}"
legacy_root="/srv/jenkins"
legacy_env_file="/srv/jenkins-runtime/stack.env"

ssh "$ssh_target" "mkdir -p '$remote_root'"

rsync -az --delete \
  --exclude '.env' \
  --exclude 'data/' \
  --exclude '__pycache__/' \
  --exclude '*.pyc' \
  "$(cd "$(dirname "$0")" && pwd)/" \
  "$ssh_target:$remote_root/"

ssh "$ssh_target" "
  set -euo pipefail
  mkdir -p '$remote_root' '$remote_root/data/controller_home' '$remote_root/data/agent_rust_linux_ci'
  if [[ ! -f '$remote_env_file' && -f '$legacy_env_file' ]]; then
    cp '$legacy_env_file' '$remote_env_file'
  fi
  if [[ '$remote_root' != '$legacy_root' && -d '$legacy_root/data/controller_home' && ! -e '$remote_root/data/controller_home/config.xml' ]]; then
    rsync -a '$legacy_root/data/controller_home/' '$remote_root/data/controller_home/'
  fi
  if [[ '$remote_root' != '$legacy_root' && -d '$legacy_root/data/agent_linux_ci' && -z \$(find '$remote_root/data/agent_rust_linux_ci' -mindepth 1 -print -quit) ]]; then
    rsync -a '$legacy_root/data/agent_linux_ci/' '$remote_root/data/agent_rust_linux_ci/'
  fi
  if [[ -d '$remote_root/data/controller_home/jobs/Rldyourterm/jobs/PR-Validation' && -d '$remote_root/data/controller_home/jobs/rldyourterm-pr' ]]; then
    rm -rf '$remote_root/data/controller_home/jobs/rldyourterm-pr'
  fi
  chown -R 1000:1000 '$remote_root/data/controller_home'
  chown -R 1001:1001 '$remote_root/data/agent_rust_linux_ci'
  if [[ '$remote_root' != '$legacy_root' && -f '$legacy_root/compose.yaml' && -f '$legacy_env_file' ]]; then
    cd '$legacy_root' && docker compose --env-file '$legacy_env_file' down || true
  fi
  cd '$remote_root' && docker compose --env-file '$remote_env_file' up -d --build
"
