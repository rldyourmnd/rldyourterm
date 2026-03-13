// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

def shellEscape(String value) {
    return (value ?: '').replace("'", "'\"'\"'")
}

def githubSetCommitStatus(String repoFullName, String sha, String context, String state, String description, String targetUrl) {
    String escapedContext = shellEscape(context)
    String escapedState = shellEscape(state)
    String escapedDescription = shellEscape(description)
    String escapedTargetUrl = shellEscape(targetUrl)

    sh(
        script: """#!/usr/bin/env bash
set -euo pipefail
payload=\$(jq -cn \\
  --arg state '${escapedState}' \\
  --arg context '${escapedContext}' \\
  --arg description '${escapedDescription}' \\
  --arg target_url '${escapedTargetUrl}' \\
  '{state: \$state, context: \$context, description: \$description, target_url: \$target_url}')
curl -fsSL -X POST \\
  -H 'Accept: application/vnd.github+json' \\
  -H "Authorization: Bearer \$GITHUB_TOKEN" \\
  -H 'X-GitHub-Api-Version: 2022-11-28' \\
  --data "\$payload" \\
  'https://api.github.com/repos/${repoFullName}/statuses/${sha}' >/dev/null
"""
    )
}

pipeline {
    agent { label 'linux-ci' }

    options {
        timestamps()
        ansiColor('xterm')
        timeout(time: 120, unit: 'MINUTES')
        buildDiscarder(logRotator(numToKeepStr: '30', artifactNumToKeepStr: '30'))
    }

    parameters {
        string(name: 'REPO_FULL_NAME', defaultValue: 'rldyourmnd/rldyourterm', trim: true)
        string(name: 'PR_NUMBER', defaultValue: '', trim: true)
        string(name: 'TRIGGER_EVENT', defaultValue: 'manual', trim: true)
        string(name: 'TRIGGER_ACTION', defaultValue: '', trim: true)
        string(name: 'TRIGGER_ACTOR', defaultValue: '', trim: true)
        text(name: 'TRIGGER_COMMENT', defaultValue: '')
    }

    environment {
        ALLOWED_GITHUB_LOGIN = 'rldyourmnd'
        REPORT_ROOT = 'target/terminal-benchmark/jenkins'
        CARGO_TERM_COLOR = 'always'
        CARGO_REGISTRIES_CRATES_IO_PROTOCOL = 'sparse'
        CARGO_NET_RETRY = '5'
        CARGO_INCREMENTAL = '0'
        RUSTFLAGS = '-D warnings'
    }

    stages {
        stage('Resolve PR Metadata') {
            steps {
                script {
                    if (!params.PR_NUMBER?.trim()) {
                        error('PR_NUMBER is required')
                    }
                }

                withCredentials([string(credentialsId: 'github-token', variable: 'GITHUB_TOKEN')]) {
                    script {
                        def prFields = sh(
                            returnStdout: true,
                            script: """#!/usr/bin/env bash
set -euo pipefail
for attempt in \$(seq 1 15); do
  response=\$(curl -fsSL \\
    -H 'Accept: application/vnd.github+json' \\
    -H "Authorization: Bearer \$GITHUB_TOKEN" \\
    -H 'X-GitHub-Api-Version: 2022-11-28' \\
    'https://api.github.com/repos/${params.REPO_FULL_NAME}/pulls/${params.PR_NUMBER}')
  mergeable=\$(jq -r '.mergeable' <<<"\$response")
  if [[ "\$mergeable" != "null" ]]; then
    jq -r '[.head.sha, .head.ref, .base.ref, .user.login, .html_url, .title, (.mergeable | tostring), .mergeable_state] | @tsv' <<<"\$response"
    exit 0
  fi
  sleep 2
done

echo 'timed out waiting for GitHub to compute pull request mergeability' >&2
exit 1
"""
                        ).trim().split('\\t')

                        env.PR_HEAD_SHA = prFields[0]
                        env.PR_HEAD_REF = prFields[1]
                        env.PR_BASE_REF = prFields[2]
                        env.PR_AUTHOR_LOGIN = prFields[3]
                        env.PR_HTML_URL = prFields[4]
                        env.PR_TITLE = prFields[5]
                        env.PR_MERGEABLE = prFields[6]
                        env.PR_MERGEABLE_STATE = prFields[7]

                        currentBuild.displayName = "#${env.BUILD_NUMBER} PR-${params.PR_NUMBER} ${env.PR_HEAD_SHA.take(7)}"
                        currentBuild.description = "${params.TRIGGER_EVENT}:${params.TRIGGER_ACTION} by ${params.TRIGGER_ACTOR} -> ${env.PR_HTML_URL}"

                        if ((params.TRIGGER_ACTOR ?: '').trim()) {
                            if (params.TRIGGER_ACTOR != env.ALLOWED_GITHUB_LOGIN) {
                                error("trigger actor '${params.TRIGGER_ACTOR}' is not allowed")
                            }
                        }

                        if (params.TRIGGER_EVENT == 'pull_request' && env.PR_AUTHOR_LOGIN != env.ALLOWED_GITHUB_LOGIN) {
                            error("PR author '${env.PR_AUTHOR_LOGIN}' is not allowed for automatic pull_request Jenkins execution")
                        }

                        if (env.PR_MERGEABLE != 'true') {
                            error("PR #${params.PR_NUMBER} is not mergeable (state=${env.PR_MERGEABLE_STATE})")
                        }
                    }
                }
            }
        }

        stage('Checkout PR Merge Ref') {
            steps {
                deleteDir()
                script {
                    sh """#!/usr/bin/env bash
set -euo pipefail
git init .
git remote add origin https://github.com/${params.REPO_FULL_NAME}.git
git fetch --depth=1 origin +refs/pull/${params.PR_NUMBER}/merge:refs/remotes/origin/pr/${params.PR_NUMBER}/merge
git checkout --force refs/remotes/origin/pr/${params.PR_NUMBER}/merge
"""
                    env.PR_CHECKOUT_SHA = sh(
                        returnStdout: true,
                        script: """#!/usr/bin/env bash
set -euo pipefail
git rev-parse HEAD
"""
                    ).trim()
                }
            }
        }

        stage('Run Extended Validation') {
            steps {
                withCredentials([string(credentialsId: 'github-token', variable: 'GITHUB_TOKEN')]) {
                    script {
                        String runner = fileExists('scripts/ci/run_jenkins_pr_ci.sh') ? 'bash scripts/ci/run_jenkins_pr_ci.sh' : 'bash /opt/jenkins/support/run_pr_ci.sh'
                        def validations = [
                            [
                                context: 'Jenkins Extended Validation',
                                mode: 'extended',
                                reportRoot: "${env.REPORT_ROOT}/extended",
                                pendingDescription: 'Jenkins extended validation is running',
                                successDescription: 'Jenkins extended validation passed',
                                failureDescription: 'Jenkins extended validation failed',
                            ],
                        ]
                        def failures = []

                        validations.each { validation ->
                            githubSetCommitStatus(
                                params.REPO_FULL_NAME,
                                env.PR_HEAD_SHA,
                                validation.context,
                                'pending',
                                validation.pendingDescription,
                                env.BUILD_URL
                            )
                        }

                        validations.each { validation ->
                            String supersededDescription = "Jenkins ${validation.context} validation was superseded by a newer run"

                            String state = 'success'
                            String description = validation.successDescription

                            try {
                                withEnv([
                                    "JENKINS_PR_TITLE=${env.PR_TITLE}",
                                    "JENKINS_REPO_FULL_NAME=${params.REPO_FULL_NAME}",
                                    "JENKINS_PR_HEAD_SHA=${env.PR_HEAD_SHA}",
                                    "JENKINS_PR_CHECKOUT_SHA=${env.PR_CHECKOUT_SHA}",
                                    "JENKINS_PR_NUMBER=${params.PR_NUMBER}",
                                    "JENKINS_TRIGGER_EVENT=${params.TRIGGER_EVENT}",
                                    "JENKINS_TRIGGER_ACTION=${params.TRIGGER_ACTION}",
                                    "JENKINS_TRIGGER_ACTOR=${params.TRIGGER_ACTOR}",
                                        ]) {
                                    sh "${runner} ${validation.mode} '${validation.reportRoot}'"
                                }
                            } catch (org.jenkinsci.plugins.workflow.steps.FlowInterruptedException err) {
                                state = 'pending'
                                description = supersededDescription
                                echo "Validation stage ${validation.context} was interrupted by a newer Jenkins run"
                                throw err
                            } catch (err) {
                                state = 'failure'
                                description = validation.failureDescription
                                failures << validation.context
                                echo "Validation stage ${validation.context} failed: ${err}"
                            } finally {
                                githubSetCommitStatus(
                                    params.REPO_FULL_NAME,
                                    env.PR_HEAD_SHA,
                                    validation.context,
                                    state,
                                    description,
                                    env.BUILD_URL
                                )
                            }
                        }

                        if (!failures.isEmpty()) {
                            error("Validation failures: ${failures.join(', ')}")
                        }
                    }
                }
            }
        }
    }

    post {
        always {
            archiveArtifacts(
                artifacts: 'target/terminal-benchmark/**/*.json, target/terminal-benchmark/**/*.sarif, target/terminal-benchmark/**/*.csv, target/terminal-benchmark/**/*.md, target/terminal-benchmark/**/*.env, scripts/mvp/output/**/*',
                allowEmptyArchive: true
            )
        }
    }
}
