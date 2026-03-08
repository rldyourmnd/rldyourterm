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
- `ui`: `crates/ui` (runtime command handling and state transitions over services).
- `app`: `crates/app` (CLI + GUI/TTY runtime wiring).
  - Internal runtime helpers are being narrowed by responsibility: shared input/palette/PTY/shutdown/terminal pieces live under `crates/app/src/runtime_shared/`; GUI backend sequencing lives in `crates/app/src/gui_runtime_backend.rs`; GUI render/deferred-GPU-init/present helpers live in `crates/app/src/gui_runtime_render.rs`; GUI PTY output pumps/backpressure/drain budgeting live in `crates/app/src/gui_runtime_output.rs`; GUI lifecycle/output-application/shutdown helpers live in `crates/app/src/gui_runtime_lifecycle.rs`; GUI window/bootstrap/viewport/cadence helpers live in `crates/app/src/gui_runtime_window.rs`; GUI terminal input/palette/clipboard/PTY-write boundary helpers live in `crates/app/src/gui_runtime_terminal_io.rs`; TTY poll/raw-mode/size control helpers live in `crates/app/src/pty_runtime_control.rs`; TTY stdout read-pump/flush/join helpers live in `crates/app/src/pty_runtime_output.rs`; TTY palette/PTY-boundary/event-disconnect helpers live in `crates/app/src/pty_runtime_terminal_io.rs`.

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
