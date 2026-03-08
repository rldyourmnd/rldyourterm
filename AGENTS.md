# AGENTS.md - rldyourterm Runtime Constitution (v1.1.0-planning-reset)

## 0) Scope
- Applies to the whole repository.
- This is the highest project-level runtime policy for Codex sessions.
- Any implementation decision must remain consistent with this file.

## 1) Decision Precedence
Use this order if documents disagree:
1. `AGENTS.md`
2. `planning/discovery/v1.0.0-answer-lock.md`
3. ADRs in `planning/adr/*`
4. Architecture and integration contracts in `planning/architecture/*` and `planning/stack/*`
5. Quality/risk/operations/roadmap docs in `planning/quality/*`, `planning/risk/*`, `planning/operations/*`, `planning/roadmap/*`
6. System governance docs in `planning/system/*`

Operational meta-rules are defined in:
- `planning/system/source-of-truth-and-precedence-v1.0.0.md`

## 2) Product Priorities (Hard Order)
1. `СТАБИЛЬНОСТЬ (CRASH-INTOLERANT)`
- Primary objective: no known crash path in normal and degraded runtime states.
- Recoverable GPU/PTY/runtime faults must not terminate active shell sessions.
- Any panic/crash is `Sev-0` and blocks release.
- Bounded retry, deterministic fallback, and event-correlated observability at each critical boundary are mandatory.

2. `AI CLI COMPATIBILITY`
- First-class compatibility for long-running workloads in:
  - `Claude Code`
  - `Codex`
  - `Gemini CLI`
  - `OpenCode`
- Deterministic prompt/input/output behavior under long sessions.
- Minimal operational noise during agent-driven automation.

3. `СКОРОСТЬ (ULTRA-LOW LATENCY)`
- Minimize input-to-render latency and shell round-trip latency.
- Keep CPU/RAM growth bounded under sustained load.
- Preserve stable frame pacing and responsive rendering during monitor transfers and burst workloads.

## 3) Product Persona and Platform Scope
- Primary user (v1.0): visual product owner and AI CLI power-user.
- Shell baseline: `fish + starship`; fallback: `zsh`.
- Bash is not a primary v1.0 shell target on Linux/macOS.
- v1.0 targets: Ubuntu 22.04 LTS, 24.04 LTS, 25.10, macOS.
- Windows in v1.0: architectural skeleton only (no full parity claim).

## 4) VSA Architecture Model (Mandatory)
Layers:
- `foundation`: OS/platform adapters and external integration ports.
- `core`: terminal domain model (grid/state/parser/events), no OS API deps.
- `services`: orchestration, lifecycle, fallback/retry, pacing/control logic.
- `features`: modular capabilities (`render_cpu`, `render_gpu`, `settings`, `shell_integration`, `diagnostics`).
- `ui`: visual/input behavior over service contracts.
- `app`: CLI, runtime bootstrap, binary packaging.

Dependency flow is strictly inward:
- `app -> features -> services -> core`
- `foundation` is consumed via explicit API traits/ports.
- Direct import of `core` into platform adapters is forbidden.

## 5) Runtime Invariants (Non-Negotiable)
- Render modes: `cpu`, `gpu`, `auto`.
- In `auto`: GPU first, bounded retry, deterministic `gpu -> cpu` fallback.
- No silent fallback: transition must be logged with event correlation and user-visible notification.
- Settings primary UX is in-terminal command palette (`Ctrl/Cmd + Shift + P`).
- Single-window baseline for v1.0 (no multiplexer/multi-window feature scope).
- Debug diagnostics mode is opt-in and observable via events.
- Scrollback default cap: 50_000 lines.

Frame pacing invariant:
- Primary path is monitor-driven cadence from system monitor timing.
- No hardcoded FPS target in the primary runtime path.
- Window transfer between displays with different refresh rates (e.g., 144Hz <-> 60Hz) must re-sync cadence without session drop.

## 6) Self-Authored-First Engineering Policy
- By default, performance-critical runtime behavior must be implemented in project-owned code.
- External crates are allowed only where direct OS/GPU/PTY integration is required or where safety/correctness would regress if reimplemented.
- Using third-party terminal-core engines is forbidden in v1.0.
- Every external dependency must have a written contract and explicit fallback/degradation semantics.

## 7) External Dependency Policy (Context7 First)
Before changing dependency-driven contracts or behavior, use Context7 against authoritative docs and update evidence docs.

Authoritative dependency references:
- `portable-pty` (`/websites/rs_portable-pty`): PTY lifecycle (`openpty`, `spawn_command`, `try_clone_reader`, `take_writer`, `resize`, `wait/kill`).
- `winit` (`/websites/rs_winit_winit`): window/event model (`Moved`, `Resized`, `ScaleFactorChanged`, `RedrawRequested`) and monitor APIs.
- `wgpu` (`/websites/rs_wgpu`): `SurfaceConfiguration`, `present_mode`, frame latency hints, surface error handling.

Evidence registry:
- `planning/system/dependency-evidence-context7-v1.0.0.md`

## 8) Planning Knowledge System (Codex Workflow)
Start each serious development session with:
1. `AGENTS.md`
2. `planning/README.md`
3. `planning/system/codex-session-playbook-v1.0.0.md`
4. `planning/system/traceability-matrix-v1.0.0.md`
5. `bash planning/system/validate_planning.sh`

If validation fails, fix planning/docs consistency before coding.

## 9) Documentation and Change Governance
Any architecture/runtime behavior change must be synchronized across docs in this order:
1. ADR/contracts (`planning/adr`, `planning/architecture`, `planning/stack`)
2. Quality/risk/operations/roadmap (`planning/quality`, `planning/risk`, `planning/operations`, `planning/roadmap`)
3. System layer (`planning/system/traceability-matrix-v1.0.0.md`, `planning/system/gap-closure-register-v1.0.0.md`)

Do not leave unresolved placeholders in authoritative docs.

## 10) Quality and Release Governance
- v1.0 release authority remains manual.
- CI quality/gov/security workflows are allowed and recommended as merge-time and pre-release validation gates, but passing CI does not by itself authorize release.
- Release readiness requires completed artifacts:
  - `planning/quality/v1.0.0-quality-gates.md`
  - `planning/quality/v1.0.0-acceptance-matrix.md`
  - `planning/operations/v1.0.0-manual-test-plan.md`
  - `planning/operations/v1.0.0-release-pack.md`
- Start-gate authority for coding:
  - `planning/operations/v1.0.0-start-readiness-index.md`

## 11) Commit and Collaboration Rules
- Keep commits small, thematic, and traceable.
- Include requirement references (`Req: R-XX`) when applicable.
- Prefer multiple focused commits over one large mixed commit.
- Never push partial architecture changes without aligned docs updates.

## 12) Non-Goals for v1.0
- Full multiplexer mode.
- Full multi-window user scenarios.
- Heavy visual effects (blur/shadow/complex gradients) as baseline behavior.
- External config-file-first UX.
- Full Windows runtime parity.

## 13) Practical Rule Of Thumb
If a change could impact session stability, fallback behavior, monitor-transfer pacing, AI CLI compatibility, or shell continuity, treat it as architecture-sensitive and update ADR/contracts/tests docs before code merge.
