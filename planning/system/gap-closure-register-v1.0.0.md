# Gap Closure Register (v1.0.0 reset sync)

| Gap ID | Problem | Resolution | Status |
|---|---|---|---|
| G-001 | Legacy docs referenced removed `metrics/version/*` layer after repository reset. | Precedence and governance moved to planning/system layer; all mandatory references updated. | Closed |
| G-002 | Traceability contained stale `crates/*` implementation links after code reset. | Traceability now points only to active planning contracts and evidence artifacts. | Closed |
| G-003 | Validation script searched non-existent `metrics` path and produced false failure. | `validate_planning.sh` updated to planning/AGENTS/README scope with stable markers. | Closed |
| G-004 | Product goal wording was weaker than new vision (crash-intolerant + AI CLI-first + ultra-low latency). | Updated AGENTS, answer-lock, quality gates, acceptance matrix, roadmap and readiness docs. | Closed |
| G-005 | Self-authored-first policy was implicit and inconsistent across docs. | Policy formalized in AGENTS, discovery lock, stack contracts, risk and traceability artifacts. | Closed |
| G-006 | Requirement IDs drifted: acceptance referenced undefined `R-16`, and validator enforced only `R-01..R-12` despite active `R-13/R-14`. | Acceptance and quality docs aligned to `R-01..R-14`; validator updated to enforce `R-01..R-14` and reject unexpected Req IDs in authoritative docs. | Closed |
| G-007 | Manual test and readiness docs used stale `run_profile` examples with `repeat=2` and old cadence command names after harness hardening (`repeat >= 3`, monitor-transfer cadence token). | Updated operations docs to `repeat >= 3`, migrated cadence examples to `transfer-cadence:*`, and aligned readiness checks with workspace-level quality gate commands. | Closed |
| G-008 | Wave-5 backlog high/critical runtime items remained open for PTY recovery, GPU fallback observability, cadence resync, runtime palette, and shell baseline determinism. | Implemented wave-6 runtime convergence commits (`7526520`..`8d9096f`) with passing package-level checks/tests and explicit requirement references (`R-01..R-13`). | Closed |
| G-009 | VSA dependency direction and diagnostics envelope contract were still inconsistent (`app/ui -> core`, feature diagnostics missing canonical foundation envelope fields). | Removed direct `core` dependency from `app/ui` via `services` re-export and aligned feature diagnostics to canonical foundation envelope (`kind/severity/layer/correlation_id`) in commit `8d86b98`. | Closed |

## Rule

Every new planning inconsistency discovered in v1.0 must be recorded here with explicit closure action and status.
