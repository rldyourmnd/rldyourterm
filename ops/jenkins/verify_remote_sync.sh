#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: verify_remote_sync.sh <ssh-target> [remote-root]

Verifies that deployed Jenkins control-plane artifacts on the target host match
the local repository sources for the rldyourterm Jenkins stack.
USAGE
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 1
fi

ssh_target="$1"
remote_root="${2:-/srv/rldyourterm-jenkins}"

if ! ssh -o BatchMode=yes -o ConnectTimeout=12 "$ssh_target" "
  [ -d '$remote_root' ] && ( [ -f '$remote_root/compose.yaml' ] || [ -f '$remote_root/docker-compose.yaml' ] || [ -f '$remote_root/docker-compose.yml' ] )
"; then
  echo "remote Jenkins root '$remote_root' is missing compose file or directory on $ssh_target" >&2
  exit 1
fi

declare -A files_to_check=(
  [casc]="ops/jenkins/controller/casc/jenkins.yaml /opt/jenkins/casc/jenkins.yaml"
  [pipeline_job]="ops/jenkins/jobs/pr-validation.groovy /opt/jenkins/jobs/pr-validation.groovy"
  [run_pr_ci]="ops/jenkins/controller/support/run_pr_ci.sh /opt/jenkins/support/run_pr_ci.sh"
)

missing=0

for key in "${!files_to_check[@]}"; do
  read -r local_path remote_path <<<"${files_to_check[$key]}"
  local_hash="$(sha256sum "$local_path" | awk '{print $1}')"

  remote_hash=""
  remote_hash="$(ssh -o BatchMode=yes -o ConnectTimeout=12 "$ssh_target" "docker exec rldyourterm-jenkins-controller sha256sum '$remote_path'" </dev/null | awk '{print $1}')"

  if [[ -z "$remote_hash" ]]; then
    echo "missing remote hash for $key ($remote_path)" >&2
    missing=1
    continue
  fi

  if [[ "$local_hash" != "$remote_hash" ]]; then
    echo "hash drift detected for $key ($remote_path): local=$local_hash remote=$remote_hash" >&2
    missing=1
  else
    echo "ok: $key"
  fi
done

controller_status="$(ssh -o BatchMode=yes -o ConnectTimeout=12 "$ssh_target" "docker ps --filter name=rldyourterm-jenkins-controller --filter status=running --format '{{.Status}}'" </dev/null)"
agent_status="$(ssh -o BatchMode=yes -o ConnectTimeout=12 "$ssh_target" "docker ps --filter name=rldyourterm-jenkins-agent-rust-linux-ci --filter status=running --format '{{.Status}}'" </dev/null)"
router_status="$(ssh -o BatchMode=yes -o ConnectTimeout=12 "$ssh_target" "docker ps --filter name=rldyourterm-jenkins-webhook-router --filter status=running --format '{{.Status}}'" </dev/null)"

if [[ -z "$controller_status" || -z "$agent_status" || -z "$router_status" ]]; then
  echo "one or more Jenkins services are not running on $ssh_target" >&2
  missing=1
else
  echo "services: controller=$controller_status agent=$agent_status router=$router_status"
fi

if [[ "$missing" -ne 0 ]]; then
  echo "remote Jenkins sync verification failed" >&2
  exit 1
fi

echo "remote Jenkins sync verification passed"
