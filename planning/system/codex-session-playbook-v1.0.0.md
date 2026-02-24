# Codex Session Playbook (v1.0.0)

## Purpose

Defines a deterministic workflow for every Codex development session so context does not drift.

## Session Start (Mandatory)

1. Read `AGENTS.md`.
2. Read `planning/discovery/v1.0.0-answer-lock.md`.
3. Read `planning/system/source-of-truth-and-precedence-v1.0.0.md`.
4. Read `planning/system/traceability-matrix-v1.0.0.md`.
5. Run:

```bash
bash planning/system/validate_planning.sh
```

If validation fails, fix docs before implementation.

## Implementation Planning Protocol

1. Identify impacted `Req ID` from traceability matrix.
2. Identify impacted layer boundaries (`foundation/core/services/features/ui/app`).
3. Identify affected ADR/contracts.
4. Identify affected risk entries and expected test evidence.
5. Define smallest safe implementation slice.

## Coding Protocol

1. Keep dependency flow compliant: `app -> features -> services -> core`.
2. Keep OS specifics behind `foundation/api` and platform adapters.
3. Preserve fallback/diagnostics invariants.
4. Avoid introducing hardcoded frame-rate values in primary render path.

## Documentation Update Protocol

Update docs in this order:

1. ADR/contracts (`planning/adr`, `planning/architecture`, `planning/stack`).
2. Quality/risk/operations docs.
3. Metrics mirror docs in `metrics/version/1.0.0`.
4. `planning/system/traceability-matrix-v1.0.0.md` and `gap-closure-register`.

## Commit Protocol

1. Use small thematic commits.
2. One commit per logical documentation slice or implementation slice.
3. Every commit message must explain intent and impacted requirement IDs.

Recommended format:

```text
<scope>: <summary>

Req: R-XX[, R-YY]
Docs: <key files>
```

## Session End

1. Re-run `bash planning/system/validate_planning.sh`.
2. Confirm changed docs still respect precedence and traceability.
3. Confirm no unresolved TODO/TBD placeholders in authoritative docs.
4. Update `planning/system/gap-closure-register-v1.0.0.md`.
