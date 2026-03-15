# Jenkins (rldyourterm) operations

This repository controls the project-scoped Jenkins stack at
`/srv/rldyourterm-jenkins`.

## Sync stack to `curestry`

```bash
bash ops/jenkins/deploy_remote.sh curestry /srv/rldyourterm-jenkins /srv/rldyourterm-jenkins/.env
```

The deploy script:
- syncs `ops/jenkins/` to the remote root,
- rebuilds containers,
- avoids active redeploys unless `JENKINS_DEPLOY_FORCE=1`,
- verifies remote hash parity of critical Jenkins artifacts by default.

Disable verification if needed:

```bash
JENKINS_VERIFY_REMOTE_SYNC=0 bash ops/jenkins/deploy_remote.sh curestry
```

## Post-deploy verification (fast)

```bash
bash ops/jenkins/verify_remote_sync.sh curestry
```

This checks:
- `ops/jenkins/compose.yaml` -> active compose file in remote root
- `ops/jenkins/controller/Dockerfile` -> `controller/Dockerfile`
- `ops/jenkins/agents/rust-linux-ci/Dockerfile` -> `agents/rust-linux-ci/Dockerfile`
- `ops/jenkins/router/Dockerfile` -> `router/Dockerfile`
- `ops/jenkins/router/router.py` -> `/app/router.py` in webhook-router
- `ops/jenkins/router/repositories.json` -> `/app/repositories.json` in webhook-router
- `ops/jenkins/controller/casc/jenkins.yaml` -> `/opt/jenkins/casc/jenkins.yaml`
- `ops/jenkins/jobs/pr-validation.groovy` -> `/opt/jenkins/jobs/pr-validation.groovy`
- `ops/jenkins/controller/support/run_pr_ci.sh` -> `/opt/jenkins/support/run_pr_ci.sh`

The command validates that `remote-root` resolves to the rldyourterm stack
(service names are hard-coded in this repository). It is safe to run for both
`/srv/rldyourterm-jenkins` and any same-layout stack, but it will fail fast
if the target compose file does not match this project.

The command also validates that controller, agent, and webhook-router containers
are running.

## PR validation modes

- `pull_request` events from GitHub trigger `ci` mode by default.
- `issue_comment` events with `@jenkins` trigger keep `extended` mode.
- You can override with the Jenkins job parameter `VALIDATION_MODE`:
  `ci`, `extended`, `codeql`, or `scorecard`.

For adaptive behavior, `ops/jenkins/jobs/pr-validation.groovy` keeps the
status context `Jenkins Extended Validation` for all modes. The mode is encoded
in the status description (`Jenkins ci/extended/codeql/scorecard validation ...`)
for operational visibility.

In `extended` mode, Jenkins now runs the following suites in parallel:
- `extended` (terminal and rust core checks)
- `codeql`
- `scorecard`

Each parallel suite uses an isolated `CARGO_TARGET_DIR` to reduce cross-suite
cache collisions in shared agent workspaces.

Behavior guarantees:
- `Jenkins Extended Validation` is reported once after all suite updates are finished.
- `pending` at suite level is used only for supersession scenarios when a newer run supersedes the current build.
- All other flow interruptions, including `timeout`, are treated as `failure` for stricter gating.
- `parallel(..., failFast: false)` is used to ensure every suite publishes a status before aggregation.

## Notes

- Do not print credentials or token values in logs.
- Keep repository secrets only in the remote `.env` file.
