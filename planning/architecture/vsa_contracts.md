# VSA Contract Notes (Research Stage)

## Layer Contract
- `foundation/`: OS/desktop integrations, PTY adapters, font loaders, clipboard, windowing system hooks, monitor timing probes.
- `core/`: terminal engine state machine, buffers, parser boundaries, key protocols.
- `services/`: orchestration: session lifecycle, mode switching, monitor-driven render pacing, failover.
- `features/`: pluggable capabilities (`render_cpu`, `render_gpu`, `settings`, `shell_integration`, `diagnostics`).
- `app/`: binaries and CLI assembly.

Детализированный контракт и контрольные ограничения: `planning/architecture/vsa-layer-contracts-v1.0.0.md`.

## Dependency Rule
- Dependencies flow inward: `app -> features -> services -> core`.
- `foundation` only appears as adapter implementations behind explicit traits.

## Stability Contract
- Any adapter failure must be contained at its boundary.
- Recovery policy belongs to services, not core.

Официальный ADR по этой теме: `planning/architecture/adr-0001-layer-boundaries.md`.
