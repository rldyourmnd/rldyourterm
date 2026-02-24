# Gap Closure Register (v1.0.0)

## Purpose

Track documentation inconsistencies, closures, and residual risk to keep planning context stable for Codex.

## Closed Gaps

| Gap ID | Problem | Resolution | Status |
|---|---|---|---|
| G-001 | Inconsistent frame-rate commands (`fps-target` variants) across docs. | Unified to monitor-driven cadence policy; command model aligned to `render cadence monitor-auto`. | Closed |
| G-002 | Long-run validation duration conflicted (`1h` vs `30 min`). | Unified long-run baseline to `>=10 minutes` for v1.0 manual gate consistency. | Closed |
| G-003 | Monitor transfer behavior (60/144Hz) lacked contract-level coverage. | Added monitor timing API/events, pacing controller contract, risk and test coverage. | Closed |
| G-004 | Missing meta knowledge entrypoint for Codex sessions. | Added `planning/README.md` and `planning/system/*` governance docs. | Closed |
| G-005 | No machine-checkable planning integrity guardrail. | Added `planning/system/validate_planning.sh` + checklist. | Closed |

## Residual Gaps (Known, Acceptable For Pre-Code)

| Gap ID | Residual Concern | Action | Owner | Status |
|---|---|---|---|---|
| RZ-001 | Dependency versions are not pinned because no Rust workspace manifest exists yet. | Pin versions at scaffolding start and update stack evidence docs. | architecture/stack | Open |
| RZ-002 | Windows adapter parity is intentionally post-v1.0. | Keep compile-valid skeleton only; exclude from v1 functional gates. | foundation-platform/windows | Accepted |

## Rule

Any new inconsistency found during implementation must be logged here with a closure note in the same or next commit.
