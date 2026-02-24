# Source Of Truth And Precedence (v1.0.0)

## Scope

Defines how Codex resolves conflicts between documents in `planning/`.

## Precedence Order

1. `AGENTS.md`
2. `planning/discovery/v1.0.0-answer-lock.md`
3. ADRs in `planning/adr/`
4. Architecture and integration contracts:
- `planning/architecture/*`
- `planning/stack/*`
5. Quality/risk/operations/roadmap execution docs:
- `planning/quality/*`
- `planning/risk/*`
- `planning/operations/*`
- `planning/roadmap/*`
6. Metrics mirror docs:
- `metrics/version/1.0.0/*`

## Conflict Resolution Algorithm

1. Locate conflicting statements.
2. Apply precedence order.
3. Keep higher-precedence statement unchanged.
4. Update lower-precedence docs to align.
5. Record change in `planning/system/gap-closure-register-v1.0.0.md`.
6. Re-run `bash planning/system/validate_planning.sh`.

## Normative Invariants (Must Never Drift)

- VSA dependency flow: `app -> features -> services -> core`.
- `foundation` exposed through explicit API traits/adapters only.
- Render modes: `cpu`, `gpu`, `auto`.
- Any durable GPU failure in `auto` leads to bounded retry then deterministic `gpu -> cpu` fallback.
- No silent fallback: transition is always logged and user-visible.
- Settings primary UX is in-terminal command palette.
- Scrollback default cap is 50_000 lines.
- Frame pacing is monitor-driven (system refresh-rate), no hardcoded fps in primary path.
- Window transfer between monitors (e.g., 144Hz <-> 60Hz) must re-sync cadence without session drop.
- v1.0.0 scope: single window, no multiplexer.

## Change Control

Use focused commits and keep change scope narrow:

1. Requirement-level change: update discovery lock, then ADR/contracts/quality docs.
2. Architecture-level change: update ADR/contracts first, then quality/risk/ops.
3. Test/release change: update operations + quality acceptance references.

## Ownership By Layer

- Product constraints and priorities: `AGENTS.md`, discovery lock.
- Runtime architecture: ADR + architecture contracts.
- External dependency behavior: stack contracts + Context7 evidence.
- Readiness/release: quality/risk/operations + metrics mirror.
