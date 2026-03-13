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
controller_uid=1000
controller_gid=1000
agent_uid=1001
agent_gid=1001
deploy_force="${JENKINS_DEPLOY_FORCE:-0}"
verify_remote_sync="${JENKINS_VERIFY_REMOTE_SYNC:-1}"

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
  chown -R '$controller_uid:$controller_gid' '$remote_root/data/controller_home'
  chown -R '$agent_uid:$agent_gid' '$remote_root/data/agent_rust_linux_ci'
  if [[ '$deploy_force' != '1' && -f '$remote_env_file' ]]; then
    set -a
    # shellcheck disable=SC1090
    source '$remote_env_file'
    set +a

    controller_running=\$(docker ps --filter 'name=rldyourterm-jenkins-controller' --filter 'status=running' --format '{{.ID}}' | head -n 1)

    if [[ -n \"\${controller_running}\" && -n \${JENKINS_ADMIN_USER:-} && -n \${JENKINS_ADMIN_PASSWORD:-} && -n \${JENKINS_HOST:-} ]]; then
      active_builds=\$(curl --globoff -fsSL -u \"\${JENKINS_ADMIN_USER}:\${JENKINS_ADMIN_PASSWORD}\" \
        \"https://\${JENKINS_HOST}/job/Rldyourterm/job/PR-Validation/api/json?tree=builds[building]\" \
        | python3 -c 'import json, sys; data = json.load(sys.stdin); print(sum(1 for build in data.get(\"builds\", []) if build.get(\"building\")))')

      case \"\${active_builds}\" in
        ''|*[!0-9]*)
          echo \"unable to determine active Jenkins builds from API response: '\${active_builds}'\" >&2
          exit 1
          ;;
      esac

      if [[ \"\${active_builds}\" != '0' ]]; then
        echo \"refusing to redeploy Jenkins while \${active_builds} PR-Validation build(s) are active; rerun with JENKINS_DEPLOY_FORCE=1 to override\" >&2
        exit 1
      fi
    fi
  fi
  if [[ '$remote_root' != '$legacy_root' && -f '$legacy_root/compose.yaml' && -f '$legacy_env_file' ]]; then
    cd '$legacy_root' && docker compose --env-file '$legacy_env_file' down || true
  fi
  cd '$remote_root' && docker compose --env-file '$remote_env_file' up -d --build
"

if [[ "$verify_remote_sync" != "0" ]]; then
  "$(cd "$(dirname "$0")" && pwd)/verify_remote_sync.sh" "$ssh_target" "$remote_root"
fi
