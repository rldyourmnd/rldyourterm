# Traceability Matrix (v1.0.0 manifest sync)

## Purpose

Связывает каждое требование `R-01..R-14` с дизайном, рисками и исполняемым evidence flow.

Machine-readable release evidence anchor:
- `planning/operations/v1.0.0-evidence-manifest.json`

## Matrix

| Req ID | Requirement | Source Of Truth | Design/Contracts | Risk Link | Validation/Release Evidence |
|---|---|---|---|---|---|
| R-01 | No session drop on recoverable failures | `AGENTS.md`, `planning/discovery/v1.0.0-answer-lock.md` | `planning/adr/0003-render-fallback.md`, `planning/architecture/vsa-layer-contracts-v1.0.0.md` | Risk 1, 2 | `planning/operations/v1.0.0-manual-test-plan.md`, `bash scripts/ci/run_e2e_governance.sh --mode release` |
| R-02 | Deterministic `gpu -> cpu` fallback in `auto` mode | `planning/discovery/v1.0.0-answer-lock.md` | `planning/adr/0003-render-fallback.md`, `planning/stack/v1.0.0-module-integration-contracts.md` | Risk 1, 7 | `planning/quality/v1.0.0-acceptance-matrix.md`, manifest artifact markers in `planning/operations/v1.0.0-evidence-manifest.json` |
| R-03 | Monitor-driven frame pacing (no hardcoded fps) | `planning/discovery/v1.0.0-answer-lock.md`, `AGENTS.md` | `planning/architecture/foundation_api_contracts_v1.0.md`, `planning/stack/v1.0.0-module-integration-contracts.md` | Risk 3 | `planning/operations/v1.0.0-manual-test-plan.md`, compatibility matrix evidence in manifest |
| R-04 | Window transfer between monitors re-syncs cadence without crash | `planning/discovery/v1.0.0-answer-lock.md` | `planning/architecture/foundation_platform_adapter_guidelines_v1.0.md`, `planning/adr/0003-render-fallback.md` | Risk 3 | `planning/operations/v1.0.0-manual-test-plan.md`, manifest `profile_gemini_log` markers |
| R-05 | Settings are primary in-terminal UX | `planning/adr/0004-settings-command-palette.md` | `planning/settings/settings_palette.md`, `planning/architecture/prod-ready-terminal-system-design.md` | Risk 10 | `planning/quality/v1.0.0-quality-gates.md`, manual scenario evidence |
| R-06 | VSA boundaries are strict and enforceable | `planning/architecture/adr-0001-layer-boundaries.md` | `planning/architecture/vsa-layer-contracts-v1.0.0.md`, `planning/architecture/foundation_api_contracts_v1.0.md` | Risk 10 | `bash scripts/ci/validate_vsa_dependency_graph.sh`, `bash scripts/ci/run_e2e_governance.sh --mode ci` |
| R-07 | Shell baseline fish+starship with zsh fallback | `planning/discovery/v1.0.0-answer-lock.md` | `planning/adr/0002-pty-strategy.md`, `planning/stack/integration-architecture-2026-02-24.md` | Risk 6 | `planning/operations/v1.0.0-manual-test-plan.md`, `bash scripts/mvp/run_matrix.sh 3` |
| R-08 | Scrollback keeps deep history with dual memory bounds | `planning/discovery/v1.0.0-answer-lock.md`, `AGENTS.md` | `planning/architecture/prod-ready-terminal-system-design.md`, `planning/quality/v1.0.0-quality-gates.md` | Risk 5 | `planning/operations/v1.0.0-manual-test-plan.md`, `planning/operations/v1.0.0-release-pack.md` |
| R-09 | PTY lifecycle safety and single writer invariant | `planning/adr/0002-pty-strategy.md` | `planning/architecture/foundation_api_contracts_v1.0.md`, `planning/stack/v1.0.0-module-integration-contracts.md` | Risk 2 | `planning/operations/v1.0.0-manual-test-plan.md`, strict governance validation |
| R-10 | Structured diagnostics with event correlation | `planning/discovery/v1.0.0-answer-lock.md` | `planning/architecture/foundation_api_contracts_v1.0.md`, `planning/architecture/prod-ready-terminal-system-design.md` | Risk 7 | `planning/operations/v1.0.0-release-pack.md`, manifest codex artifact markers |
| R-11 | Single-window baseline (no multiplexer) | `AGENTS.md`, `planning/discovery/v1.0.0-answer-lock.md` | `planning/v1.0.0-development-blueprint.md`, `planning/architecture/vsa-layer-contracts-v1.0.0.md` | Risk 10 | manifest artifact markers (`windows=1`, `single_window_required=1`, `single_window_enforced=yes`) |
| R-12 | Manual-only release governance for v1.0.0 | `AGENTS.md`, `planning/v1.0.0-development-blueprint.md` | `planning/operations/v1.0.0-release-pack.md`, `planning/operations/v1.0.0-start-readiness-index.md` | Risk 8 | `bash scripts/ci/validate_release_evidence_freshness.sh --mode strict` |
| R-13 | AI CLI compatibility for Claude/Codex/Gemini is explicit release gate | `AGENTS.md`, `planning/discovery/v1.0.0-answer-lock.md` | `planning/quality/v1.0.0-quality-gates.md`, `planning/operations/v1.0.0-manual-test-plan.md` | Risk 4 | `bash scripts/mvp/run_matrix.sh 3`, matrix/profile artifacts from manifest |
| R-14 | Self-authored-first runtime policy is enforced | `AGENTS.md`, `planning/discovery/v1.0.0-answer-lock.md` | `planning/stack/v1.0.0-module-integration-contracts.md`, `planning/stack/tech-review-2026-02-24.md` | Risk 9 | `planning/system/dependency-evidence-context7-v1.0.0.md`, `bash scripts/ci/validate_vsa_dependency_graph.sh` |

## Usage

- Любое изменение governance/quality/release flow должно обновлять соответствующие строки `R-xx`.
- Для release-signoff evidence считается валидным только после strict-проверки manifest.
