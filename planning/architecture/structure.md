# VSA Architecture Notes (Implementation Sync, v1.0)

## 1) Core principle

Проект остается VSA-ориентированным с жесткими границами ответственности:

- `foundation` — platform contracts + boundary error model.
- `core` — терминальная доменная модель без OS API.
- `services` — orchestration/recovery/pacing policy.
- `features` — модульные runtime capabilities.
- `ui` — визуальное поведение поверх сервисных контрактов.
- `app` — bootstrap, CLI, runtime assembly.

## 2) Current workspace crate map (as implemented on 2026-03-04)

```text
terminal_benchmark/
crates/
  app/
  core/
  services/
  ui/
  foundation/
  foundation-platform/
  features/
    render_cpu/
    render_gpu/
    settings/
    shell_integration/
    diagnostics/
```

## 3) Layer ownership map (implementation-aligned)

- `foundation`:
  - `crates/foundation` — traits/contracts/types/errors.
  - `crates/foundation-platform` — platform implementations (`pty`, `window`, `clipboard`).
- `core`: `crates/core`.
- `services`: `crates/services` (depends on `core` + `foundation` contracts).
- `features`: `crates/features/*` (settings/render/shell/diagnostics capabilities).
  - `render_gpu` internal ownership is being narrowed as well: surface recovery/configuration contracts now live in `crates/features/render_gpu/src/surface.rs`, glyph-atlas bootstrap/upload/cache helpers live in `crates/features/render_gpu/src/atlas.rs`, pipeline-cache IO/persistence helpers live in `crates/features/render_gpu/src/pipeline_cache.rs`, cell-buffer capacity/row-preparation/dirty-range upload helpers live in `crates/features/render_gpu/src/cell_data.rs`, GPU bootstrap/pipeline/resource initialization now lives in `crates/features/render_gpu/src/bootstrap.rs`, and frame/resize/presentation orchestration now lives in `crates/features/render_gpu/src/frame.rs`, leaving `lib.rs` as the renderer-facing API root instead of the main implementation owner.
- `ui`: `crates/ui` (runtime command handling and state transitions over services).
  - `UiRuntime` root definitions stay in `crates/ui/src/lib.rs`, while command execution/state-machine helpers now live in `crates/ui/src/commands.rs`, so UI contract types stay separate from command-processing policy.
- `app`: `crates/app` (CLI + GUI/TTY runtime wiring).
  - Internal runtime helpers are being narrowed by responsibility: harness/bootstrap-command parsing, palette-command application, and MVP reporting now live in `crates/app/src/app_harness.rs`; shared input/palette/PTY/shutdown/terminal pieces live under `crates/app/src/runtime_shared/`; GUI backend sequencing lives in `crates/app/src/gui_runtime_backend.rs`; GUI render/deferred-GPU-init/present helpers live in `crates/app/src/gui_runtime_render.rs`; GUI PTY output pumps/backpressure/fixed-capacity chunk transport/drain budgeting live in `crates/app/src/gui_runtime_output.rs`; GUI lifecycle/output-application/shutdown helpers live in `crates/app/src/gui_runtime_lifecycle.rs`; GUI window/bootstrap/viewport/cadence helpers live in `crates/app/src/gui_runtime_window.rs`; GUI terminal input/palette/clipboard/PTY-write boundary helpers live in `crates/app/src/gui_runtime_terminal_io.rs`; TTY poll/raw-mode/size control helpers live in `crates/app/src/pty_runtime_control.rs`; TTY stdout read-pump/flush/join helpers live in `crates/app/src/pty_runtime_output.rs`; TTY palette/PTY-boundary/event-disconnect helpers live in `crates/app/src/pty_runtime_terminal_io.rs`.
- `terminal_benchmark`: root-level tooling crate for reproducible headless performance baselines.
  - Consumes only stable public surfaces (`services::terminal`, `render_cpu`, `font`) and does not reach into app-private runtime modules or environment-sensitive GPU/window ownership.
  - Owns deterministic workload generation, iteration stats, JSON/table reporting, and benchmark scenario composition for future perf-regression automation.

## 4) Current dependency graph (observed in crate manifests)

- `services -> {core, foundation}`
- `ui -> services`
- `features/settings -> services` (other feature crates are feature-local and/or snapshot consumers)
- `app -> {ui, services, foundation, foundation-platform, features/*}`

Normative target from `AGENTS.md` remains:

- inward flow: `app -> features -> services -> core`
- foundation integration via explicit `foundation/api` trait boundaries.

Current drift that must stay explicit:

- direct `app -> foundation` dependencies remain in runtime bootstrap paths for adapter bootstrap and platform integration.
- window ownership drift is closed: app runtime window lifecycle goes through foundation window contracts.

## 5) Foundation adapter runtime status (W3 ownership sync)

- `foundation-platform::pty` is runtime-wired in GUI/TTY flows.
- `foundation-platform::clipboard` is runtime-wired in app path.
- `foundation-platform::window` is runtime-wired via `WindowFactory/WindowControl`.
- GUI runtime avoids direct app-owned `window.request_redraw`, `window.set_title`, `window.current_monitor` control path.

Closure evidence for `G-010`:

1. App runtimes instantiate and control window lifecycle via `foundation/api::window::{WindowFactory, WindowControl}`.
2. Monitor timing for cadence resync is sourced from foundation window contract/events.
3. Existing behavior parity remains intact: single-window enforcement, palette shortcuts flow, GPU fallback observability, monitor-transfer cadence re-sync.
