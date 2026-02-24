# Traceability Matrix (v1.0.0)

## Purpose

Ensures every critical requirement is connected to design decisions, implementation boundaries, risk ownership, and validation artifacts.

## Matrix

| Req ID | Requirement | Source Of Truth | Design/Contracts | Risk Link | Validation/Release Evidence |
|---|---|---|---|---|---|
| R-01 | No session drop on recoverable failures | `AGENTS.md`, `planning/discovery/v1.0.0-answer-lock.md` | `planning/adr/0003-render-fallback.md`, `planning/architecture/vsa-layer-contracts-v1.0.0.md` | Risk 1, 2, 3 | `planning/quality/v1.0.0-quality-gates.md`, `planning/operations/v1.0.0-manual-test-plan.md` |
| R-02 | Deterministic `gpu -> cpu` fallback in `auto` mode | `planning/discovery/v1.0.0-answer-lock.md` | `planning/adr/0003-render-fallback.md`, `planning/architecture/foundation_api_contracts_v1.0.md` | Risk 1 | `planning/quality/v1.0.0-acceptance-matrix.md`, `planning/operations/v1.0.0-manual-test-plan.md` |
| R-03 | Monitor-driven frame pacing (no hardcoded fps) | `planning/discovery/v1.0.0-answer-lock.md`, `planning/v1.0.0-development-blueprint.md` | `planning/architecture/foundation_api_contracts_v1.0.md`, `planning/stack/v1.0.0-module-integration-contracts.md` | Risk 3, 9 | `planning/quality/v1.0.0-quality-gates.md`, `planning/operations/v1.0.0-manual-test-plan.md` |
| R-04 | Correct window transfer between 60/144Hz monitors without crash | `planning/discovery/v1.0.0-requirements-questionnaire.md` | `planning/architecture/foundation_platform_adapter_guidelines_v1.0.md`, `planning/adr/0003-render-fallback.md` | Risk 3, 9 | `planning/quality/v1.0.0-acceptance-matrix.md`, `planning/operations/v1.0.0-release-pack.md` |
| R-05 | Settings are primary in-terminal UX | `planning/adr/0004-settings-command-palette.md` | `planning/settings/settings_palette.md`, `planning/architecture/prod-ready-terminal-system-design.md` | Risk 7 | `planning/quality/v1.0.0-quality-gates.md`, `planning/operations/v1.0.0-manual-test-plan.md` |
| R-06 | VSA boundaries are strict and enforceable | `planning/architecture/adr-0001-layer-boundaries.md` | `planning/architecture/vsa-layer-contracts-v1.0.0.md`, `planning/architecture/foundation_api_contracts_v1.0.md` | Cross-cutting | `planning/architecture/v1.0.0-consistency-manifest.md`, `planning/operations/v1.0.0-start-readiness-index.md` |
| R-07 | Shell baseline fish+starship with zsh fallback | `planning/discovery/v1.0.0-answer-lock.md` | `planning/adr/0002-pty-strategy.md`, `planning/stack/integration-architecture-2026-02-24.md` | Risk 6 | `planning/quality/v1.0.0-acceptance-matrix.md`, `planning/operations/v1.0.0-manual-test-plan.md` |
| R-08 | Scrollback cap 50_000 with bounded memory growth | `planning/discovery/v1.0.0-answer-lock.md` | `planning/architecture/prod-ready-terminal-system-design.md`, `planning/quality/v1.0.0-quality-gates.md` | Risk 5 | `planning/quality/v1.0.0-acceptance-matrix.md`, `metrics/version/1.0.0/quality_targets.md` |
| R-09 | PTY lifecycle safety and single writer invariant | `planning/adr/0002-pty-strategy.md` | `planning/architecture/foundation_api_contracts_v1.0.md`, `planning/stack/v1.0.0-module-integration-contracts.md` | Risk 2 | `planning/operations/v1.0.0-manual-test-plan.md` |
| R-10 | Structured diagnostics with event correlation | `planning/discovery/v1.0.0-answer-lock.md` | `planning/architecture/foundation_api_contracts_v1.0.md`, `planning/architecture/prod-ready-terminal-system-design.md` | Risk 1, 2, 3, 9 | `planning/quality/v1.0.0-quality-gates.md`, `planning/operations/v1.0.0-release-pack.md` |
| R-11 | Single-window baseline (no multiplexer) | `AGENTS.md`, `planning/discovery/v1.0.0-answer-lock.md` | `planning/v1.0.0-development-blueprint.md`, `planning/architecture/vsa-layer-contracts-v1.0.0.md` | Scope | `planning/quality/v1.0.0-acceptance-matrix.md` |
| R-12 | Manual-only release governance for v1.0.0 | `planning/v1.0.0-development-blueprint.md` | `planning/operations/v1.0.0-release-pack.md`, `planning/operations/v1.0.0-start-readiness-index.md` | Risk 8 | `metrics/version/1.0.0/definition_of_done.md` |

## Usage

When implementing any feature, reference at least one `Req ID` from this matrix in the task notes and update affected rows if scope changes.
