#!/usr/bin/env bash
set -euo pipefail

: "${JENKINS_URL:?JENKINS_URL is required}"
: "${JENKINS_ADMIN_USER:?JENKINS_ADMIN_USER is required}"
: "${JENKINS_ADMIN_PASSWORD:?JENKINS_ADMIN_PASSWORD is required}"
: "${JENKINS_AGENT_NAME:?JENKINS_AGENT_NAME is required}"

agent_workdir="${JENKINS_AGENT_WORKDIR:-/home/jenkins/agent}"
controller_url="${JENKINS_URL%/}"
jnlp_url="${controller_url}/computer/${JENKINS_AGENT_NAME}/jenkins-agent.jnlp"
agent_jar_url="${controller_url}/jnlpJars/agent.jar"

mkdir -p "${agent_workdir}"

until curl -fsS "${controller_url}/login" >/dev/null; do
  sleep 5
done

secret="$(
  curl -fsS --user "${JENKINS_ADMIN_USER}:${JENKINS_ADMIN_PASSWORD}" "${jnlp_url}" \
    | python3 -c 'import sys, xml.etree.ElementTree as ET; root = ET.parse(sys.stdin).getroot(); args = [node.text for node in root.iter("argument")]; print(args[0])'
)"

curl -fsSLo /tmp/agent.jar --user "${JENKINS_ADMIN_USER}:${JENKINS_ADMIN_PASSWORD}" "${agent_jar_url}"

exec java -jar /tmp/agent.jar \
  -url "${controller_url}/" \
  -secret "${secret}" \
  -name "${JENKINS_AGENT_NAME}" \
  -workDir "${agent_workdir}" \
  -webSocket
