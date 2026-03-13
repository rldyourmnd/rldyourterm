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
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

if command -v git >/dev/null 2>&1; then
  detected_repo_root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -n "$detected_repo_root" ]]; then
    repo_root="$detected_repo_root"
  fi
fi

required_container_names=(
  rldyourterm-jenkins-controller
  rldyourterm-jenkins-agent-rust-linux-ci
  rldyourterm-jenkins-webhook-router
)

run_remote() {
  ssh -o BatchMode=yes -o ConnectTimeout=12 "$ssh_target" "$1"
}

pick_remote_compose_file() {
  local candidate=""
  for candidate in compose.yaml docker-compose.yaml docker-compose.yml; do
    if run_remote "[ -f \"${remote_root}/${candidate}\" ]"; then
      printf '%s\n' "${remote_root}/${candidate}"
      return 0
    fi
  done
  return 1
}

if ! run_remote "[ -d \"${remote_root}\" ]"; then
  echo "remote Jenkins root '$remote_root' does not exist on $ssh_target" >&2
  exit 1
fi

remote_compose_file="$(pick_remote_compose_file || true)"
if [[ -z "$remote_compose_file" ]]; then
  echo "missing compose file in remote root '$remote_root' on $ssh_target" >&2
  exit 1
fi

remote_container_names="$(run_remote "awk '/^[[:space:]]*container_name:/ {print \$2}' \"${remote_compose_file}\"")"
for required_container in "${required_container_names[@]}"; do
  if ! grep -Fxq "$required_container" <<<"$remote_container_names"; then
    echo "remote root '$remote_root' is not the rldyourterm Jenkins stack (expected container '$required_container' in ${remote_compose_file})" >&2
    exit 1
  fi
done

declare -A local_file_for_key=(
  [casc]="$repo_root/ops/jenkins/controller/casc/jenkins.yaml"
  [pipeline_job]="$repo_root/ops/jenkins/jobs/pr-validation.groovy"
  [run_pr_ci]="$repo_root/ops/jenkins/controller/support/run_pr_ci.sh"
)

declare -A remote_file_for_key=(
  [casc]="/opt/jenkins/casc/jenkins.yaml"
  [pipeline_job]="/opt/jenkins/jobs/pr-validation.groovy"
  [run_pr_ci]="/opt/jenkins/support/run_pr_ci.sh"
)

declare -A remote_container_for_key=(
  [casc]="rldyourterm-jenkins-controller"
  [pipeline_job]="rldyourterm-jenkins-controller"
  [run_pr_ci]="rldyourterm-jenkins-controller"
)

missing=0
service_statuses=()

for required_container in "${required_container_names[@]}"; do
  status="$(run_remote "docker ps --filter \"name=^/${required_container}$\" --filter status=running --format '{{.Status}}'")"
  if [[ -z "$status" ]]; then
    echo "service container is not running: $required_container" >&2
    missing=1
  else
    service_statuses+=("$required_container=$status")
  fi
done

for key in "${!local_file_for_key[@]}"; do
  local_path="${local_file_for_key[$key]}"
  remote_container="${remote_container_for_key[$key]}"
  remote_path="${remote_file_for_key[$key]}"

  if [[ ! -f "$local_path" ]]; then
    echo "missing local Jenkins source file: $local_path" >&2
    missing=1
    continue
  fi

  local_hash="$(sha256sum "$local_path" | awk '{print $1}')"

  remote_hash="$(run_remote "docker exec '${remote_container}' sha256sum '${remote_path}'" </dev/null | awk '{print $1}')"

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

if [[ "${#service_statuses[@]}" -ne "${#required_container_names[@]}" ]]; then
  echo "one or more Jenkins services are not running on $ssh_target" >&2
  missing=1
fi

if [[ "$missing" -ne 0 ]]; then
  echo "services status checks: ${service_statuses[*]:-<none>}" >&2
  echo "remote Jenkins sync verification failed" >&2
  exit 1
fi

if [[ "${#service_statuses[@]}" -gt 0 ]]; then
  IFS=' ' read -r -a _sorted_services <<<"$(printf '%s\n' "${service_statuses[@]}" | sort | tr '\n' ' ')"
  echo "services: ${_sorted_services[*]}"
fi

echo "remote Jenkins sync verification passed"
