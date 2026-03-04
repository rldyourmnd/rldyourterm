# Planning Validation Checklist (v1.0.0 reset sync)

## Priority and Scope

- [ ] Priority order is explicit and consistent: stability -> AI CLI compatibility -> speed.
- [ ] Single-window scope is preserved and non-goals are unchanged.
- [ ] Self-authored-first policy is explicitly present in top-level governance docs.

## Runtime Invariants

- [ ] Render modes `cpu/gpu/auto` are consistent across docs.
- [ ] `gpu -> cpu` fallback behavior is deterministic and observable.
- [ ] Frame pacing is monitor-driven and does not require hardcoded fps target.
- [ ] Scrollback default cap is consistently `50_000`.
- [ ] Palette UX scope is explicit: shortcut-first surface, no free-form command-line input claimed for UI palette.

## Architecture and Integration

- [ ] VSA boundaries are consistent with dependency direction rules.
- [ ] PTY/window/GPU dependency contracts are documented and referenced.
- [ ] Current crate/layer map reflects actual workspace structure (not legacy planned paths).
- [ ] Foundation clipboard runtime integration closure status is explicit.
- [ ] Foundation window runtime integration closure is explicit and evidence-backed (`WindowFactory/WindowControl` lifecycle, cadence from foundation window contracts/events, behavior parity preserved).
- [ ] Validator is fail-fast for direct app-owned window control regressions (`window.request_redraw`, `window.set_title`, `window.current_monitor`).
- [ ] No stale source-of-truth references point to removed code artifacts.

## Quality, Risk, Operations

- [ ] Quality gates, acceptance matrix, and manual test plan are consistent.
- [ ] Risk matrix mitigations map to acceptance/release evidence.
- [ ] Release pack references required manual evidence and traceability.

## System Consistency

- [ ] Traceability matrix covers `R-01`..`R-14` at minimum.
- [ ] Authoritative planning docs include only expected Req IDs (`R-01`..`R-14`).
- [ ] Gap closure register reflects latest resolved contradictions.
- [ ] `bash planning/system/validate_planning.sh` passes.
