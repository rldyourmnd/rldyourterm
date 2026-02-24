# Planning Validation Checklist (v1.0.0)

## Usage

Run this checklist before implementation and before release freeze.

## Checklist

- [ ] `AGENTS.md` constraints are not contradicted by planning docs.
- [ ] Discovery lock remains aligned with ADR/contracts.
- [ ] All critical requirements map to `Req ID` rows in traceability matrix.
- [ ] Render policy is monitor-driven and does not require hardcoded fps target.
- [ ] 60/144Hz monitor transfer behavior is covered in contracts, risk, and tests.
- [ ] GPU fallback behavior (`gpu -> cpu`) is deterministic and observable.
- [ ] PTY single-writer and teardown contracts are explicit.
- [ ] Quality gates, acceptance matrix, and manual test plan are consistent.
- [ ] Risk matrix has owner + mitigation for each high/medium risk.
- [ ] Release pack references manual evidence and metrics docs.
- [ ] Metrics docs mirror the same v1.0.0 constraints and targets.
- [ ] No unresolved TODO/TBD/XXX placeholders in authoritative docs.

## Automated Companion

```bash
bash planning/system/validate_planning.sh
```
