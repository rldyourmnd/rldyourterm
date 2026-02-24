# System Docs For Codex (v1.0.0)

This folder contains the meta-layer that turns `planning/` into an operational knowledge system.

## Contents

- `source-of-truth-and-precedence-v1.0.0.md`: conflict resolution and normative precedence.
- `traceability-matrix-v1.0.0.md`: requirement -> ADR -> contract -> risk -> test/release mapping.
- `codex-session-playbook-v1.0.0.md`: deterministic start-to-finish flow for development sessions.
- `dependency-evidence-context7-v1.0.0.md`: authoritative external API evidence used by the design.
- `gap-closure-register-v1.0.0.md`: known gaps, closures, residual risks.
- `planning-validation-checklist-v1.0.0.md`: manual checklist for documentation quality.
- `validate_planning.sh`: automated consistency check.

## Rule

Any update that changes architecture behavior, requirements, or release criteria must also update this folder where applicable.
