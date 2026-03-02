# Planning Knowledge System (v1.0.0 reset sync)

## Purpose

`planning/` is the operational source of truth for `rldyourterm` v1.0.0.
It is designed so Codex can start a session, recover full context quickly, and make implementation decisions without ambiguity.

## Fast Start For Codex

1. Read `AGENTS.md` (hard constraints and priorities).
2. Read `planning/discovery/v1.0.0-answer-lock.md` (locked product decisions).
3. Read `planning/v1.0.0-development-blueprint.md` (execution baseline).
4. Read `planning/system/source-of-truth-and-precedence-v1.0.0.md` (conflict resolution and edit rules).
5. Read `planning/system/traceability-matrix-v1.0.0.md` (requirement-to-validation map).
6. Read `planning/operations/v1.0.0-start-readiness-index.md` (start gate).

## Directory Structure

- `planning/discovery/`: finalized requirements and lock answers.
- `planning/adr/`: accepted architectural decisions.
- `planning/architecture/`: contracts, boundaries, and implementation design.
- `planning/stack/`: dependency and integration contracts.
- `planning/quality/`: quality gates and acceptance criteria.
- `planning/risk/`: risk matrix and mitigation strategy.
- `planning/roadmap/`: staged implementation schedule.
- `planning/operations/`: manual validation and release process.
- `planning/settings/`: in-terminal settings UX contract.
- `planning/system/`: meta-knowledge docs for Codex navigation, traceability, validation, and session playbooks.

## Source-of-Truth Rules

Use this order when documents disagree:
1. `AGENTS.md`
2. `planning/discovery/v1.0.0-answer-lock.md`
3. ADR (`planning/adr/*`)
4. Layer/contracts (`planning/architecture/*`, `planning/stack/*`)
5. Execution/process (`planning/roadmap/*`, `planning/quality/*`, `planning/risk/*`, `planning/operations/*`)
6. System governance (`planning/system/*`)

Authoritative details are consolidated in:
- `planning/system/source-of-truth-and-precedence-v1.0.0.md`

## Validation

Run the planning integrity check before and after significant doc changes:

```bash
bash planning/system/validate_planning.sh
```

## Session Playbook

Use:
- `planning/system/codex-session-playbook-v1.0.0.md`

to keep every implementation session deterministic and traceable.
