# v1.0.0 Quality Targets

## Stability Targets
- session persistence during render transitions (gpu<->cpu) and resize storms;
- GPU failures recover to CPU without shell termination;
- no uncontrolled shutdown on long output/long session conditions.

## AI Workflow Targets
- deterministic prompt roundtrip under burst-like output;
- stable copy/paste and scrollback during heavy interaction;
- command palette settings remain runtime-safe and rollback-safe.

## Speed and Efficiency Targets
- bounded idle CPU wakeups (no busy polling in hot path);
- controlled transition for `auto` mode;
- default scrollback cap: 50_000 lines and bounded diagnostic event queues;
- visual stability across 60↔144Hz transitions.

## Evidence and Traceability
- Primary acceptance source: `planning/quality/v1.0.0-acceptance-matrix.md`.
- Quality gates: `planning/quality/v1.0.0-quality-gates.md`.
- Manual completion evidence: `planning/operations/v1.0.0-manual-test-plan.md`.
