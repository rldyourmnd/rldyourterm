# VSA Contract Notes (Research Stage)

## Layer Contract
- `foundation/`: OS/desktop integrations, PTY adapters, font loaders, clipboard, windowing system hooks.
- `core/`: terminal engine state machine, buffers, parser boundaries, key protocols.
- `services/`: orchestration: session lifecycle, mode switching, failover.
- `features/`: pluggable capabilities (render.cpu, render.gpu, settings.ui, shell integration).
- `app/`: binaries and CLI assembly.

Детализированный контракт и контрольные ограничения: `planning/architecture/vsa-layer-contracts-v1.0.0.md`.

## Dependency Rule
- Dependencies flow inward: `app -> features -> services -> core`.
- `foundation` only appears as adapter implementations behind explicit traits.

## Stability Contract
- Any adapter failure must be contained at its boundary.
- Recovery policy belongs to services, not core.

Официальный ADR по этой теме: `planning/architecture/adr-0001-layer-boundaries.md`.
