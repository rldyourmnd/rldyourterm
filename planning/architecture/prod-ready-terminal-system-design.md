# Prod-Ready System Design for rldyourterm v1.0

Дата: 2026-02-24

## 1) Назначение документа

Этот документ задает production-ready структуру и каркас для реализации, включая ОС-специфические адаптеры, даже если реализация на данный момент еще не завершена.

Цели:
- один терминал одно окно;
- устойчивый runtime для AI-сценариев;
- управляемый fallback для рендера и PTY;
- прозрачная диагностика всех критичных boundary errors.

## 2) Слойная модель с границами

### 2.1 Core contracts

- core: terminal state model, parser, grid, cursor, scrolling, event types.
- services: сессия, режимы рендера, retry/backoff, settings service, session health.
- features: render_cpu, render_gpu, settings, shell_integration, diagnostics.
- ui: рендер и инпут-события поверх services.
- app: CLI/конфиг/инициализация/жизненный цикл.
- foundation: PTY/window/clipboard/логирование как внешние порты.

### 2.2 Dependency rule

`app -> features -> services -> core`

`foundation` интегрируется только через `foundation/api` traits.

## 3) Prod-ready crate map (target layout to be recreated after reset)

```text
crates/
  app/
  features/
    diagnostics/
    render_cpu/
    render_gpu/
    settings/
    shell_integration/
  services/
  core/
  ui/
  foundation/
    api/
    pty/
    window/
    clipboard/
    tracing/
  foundation-platform/
    linux/
    macos/
    windows/
```

### app

- `cli.rs`: параметризация runtime, режим render и профиль.
- `main.rs`: glue + bootstrap, init logging + panic hooks.
- `runtime_config.rs`: profile schema + defaults.

### services

- `session.rs`: spawn shell, io pump, lifecycle, exit handling.
- `render_mode.rs`: `Cpu/Gpu/Auto` decision engine.
- `render_pacing.rs`: monitor-driven cadence controller (refresh-aware scheduling).
- `pty_adapter.rs`: normalized PTY errors and retries.
- `settings.rs`: command palette parsing + transactional apply.
- `diagnostics.rs`: event emitter с event-id и trace correlation.

### features

- `render_cpu/`:
  - font raster baseline, dirty rectangle renderer.
- `render_gpu/`:
  - `wgpu` surface init, pipeline, resize config, failure observer.
- `settings/`:
  - palette command parser и command dispatcher.
- `shell_integration/`:
  - fish/zsh bootstrap states и fallback.
- `diagnostics/`:
  - structured logs + sampling + optional local dump.

### core

- `grid/`: ячейки, атрибуты, dirty regions.
- `parser/`: ANSI/escape baseline subset.
- `cursor/`: позиция, visibility, mode flags.
- `state/`: terminal transitions в чистом коде.
- `scrollback/`: bounded ring with cap 50_000 lines + high byte-budget guardrail (default 512 MiB).

### ui

- render orchestration between feature backends.
- input bridge клавиатура/мышь/copy/paste.

### foundation/api

- `pty.rs`: open/write/read/resize/close.
- `window.rs`: create window, control flow, event enum.
- `clipboard.rs`: copy/paste transport.
- `diagnostics.rs`: event sink facade.

### foundation-platform

#### Linux
- PTY via `portable-pty` + сигналы/resize integration.
- Winit-based window backend.
- X11/Wayland abstraction via winit event stream.

#### macOS
- PTY via `portable-pty`.
- winit + macOS-specific lifecycle handling (focus/activation/touches).
- Font metrics differences and scale factor handling.

#### Windows (skeleton in v1.0)
- PTY via `portable-pty` (adapter stub layer, not primary production path in v1.0).
- winit window backend compatibility wrapper.
- No full user-facing parity claim in v1.0.

## 4) Рекомендуемая структура процессов

### 4.1 Flow bootstrap

1. parse argv → runtime mode + profile.
2. init tracing + diagnostics sink.
3. create foundation window adapter and event loop bridge.
4. create PTY adapter, resolve shell command.
5. start services/session and start IO pumps.
6. start chosen render backend.
7. enter main loop: event -> service -> core -> render.

### 4.2 Event sequence (happy path)

- keyboard input -> `foundation/api::window` -> `services/session` -> `core/grid` -> dirty regions -> active renderer.
- child output -> `services/session` -> `core/parser` -> `ui` redraw.
- resize -> `foundation/api::window` -> `services/render_mode` + `services/session`.
- monitor move/scale change -> `foundation/api::window` -> `services/render_pacing` -> cadence re-sync to current monitor refresh.

### 4.3 Error sequence (fallback)

- GPU error/DeviceLost -> render mode service increments failure counter -> bounded retry -> switch cpu -> `RenderModeTransition` event -> notify user -> continue session.
- PTY read/write/resize error -> service marks degraded/reattach -> retry policy -> controlled stop with no data-loss.

## 5) OS-specific configuration matrix

- Linux: `foundation-platform/linux` active target, baseline full path for v1.0.
- macOS: `foundation-platform/macos` active target, baseline full path for v1.0.
- Windows: `foundation-platform/windows` adapter skeleton, build allowed, но только ограниченная функциональная полнота.

## 6) Пороги и ограничения v1.0

- scrollback max: 50_000 lines.
- scrollback byte budget: default 512 MiB (oldest-line eviction on overflow).
- no multiplexer, no multi-window.
- no mandatory file-config-first UX.
- поддержка только baseline визуальных эффектов без heavy effects.

## 7) API boundary contracts (минимум)

### foundation/api/pty.rs

- `open_pty(cols, rows) -> PtyHandle`
- `spawn_shell(cmd, env, cwd) -> PtySession`
- `read()`/`write(bytes)`/`resize(cols, rows)`/`close()`
- `child_wait_timeout()`/`child_kill_graceful()`

### foundation/api/window.rs

- `WindowControl::request_redraw()`
- `WindowControl::set_title()`
- `WindowControl::current_monitor_timing()`
- `WindowControl::close()`
- `app` владеет `winit` event handling напрямую; foundation window boundary предоставляет только operational control + monitor timing sampling.

### foundation/api/diagnostics.rs

- `emit(DomainEvent)`
- `with_correlation(correlation_id)`
- `snapshot()`

## 8) Production hardening checklist

- bounded retries для рендера и PTY.
- отдельные timeouts для read/write/kill.
- explicit ownership для single writer.
- structured log per failure path with correlation id.
- command palette transaction + rollback.
- deterministic startup/shutdown state transitions.
- deterministic monitor transfer behavior (144Hz <-> 60Hz) with immediate cadence re-sync and no session drop.

## 9) References from authoritative sources

- `portable-pty` API shape and contracts: `/websites/rs_portable-pty`
- `winit` event-loop and window event model: `/websites/rs_winit_winit`
- `wgpu` error callbacks and error scoping: `/websites/rs_wgpu`
