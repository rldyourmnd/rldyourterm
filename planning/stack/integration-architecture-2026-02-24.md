# Integration Architecture (v1.0.0 Planning, 2026-02-24)

## Ключевой ответ на вопрос
Да, мы будем интегрироваться, но только с OS- и runtime-примититвами:
- `portable-pty` — создание и управление PTY, spawn shell, resize, read/write.
- `winit` — окно, цикл событий, клавиатура/мышь, resize/window lifecycle.
- `wgpu` — GPU рендер backend (cross-platform).
- Наши собственные `core/grid/parser/state` + `features/render_cpu` + `features/render_gpu`.

Никаких зависимостей от чужого terminal-движка как архитектурного ядра.

## Что подключаем сейчас (v1.0.0)
1. PTY Layer (`foundation/api::pty` + `foundation-platform` adapters)
- `portable-pty`
- Задачи: spawn shell, master/slave IO, resize, kill/safe shutdown.

2. Event/Window Layer (`foundation/api::window` + `foundation-platform` adapters)
- `winit`
- Задачи: create window, scale DPI, keyboard/mouse input, redraw scheduling, monitor refresh detection.

3. Render Core (`features/render_cpu`, `features/render_gpu`)
- CPU path: внутренний raster/compose path без GPU зависимостей.
- GPU path: `wgpu` + internal text cache/quad pipeline.

4. Text Pipeline (`features/render_cpu`, `features/render_gpu`)
- Начать с минимального внутреннего текстового рендера для ASCII/UTF-8 baseline.
- При росте потребности в шейпинге/ligatures перейти к модульному пути (`cosmic-text`/`glyphon`) внутри рендер features.

5. Settings UX (`features/settings`)
- In-terminal command palette + runtime config actions.

6. Shell Integration (`features/shell_integration`)
- fish + Starship bootstrap (init/activation) через адаптеры для future-свободы.
- Дополнительный fallback-путь на `zsh` для совместимости и стабильности.
- Bash не является обязательным целевым shell в v1.0.0 на Linux/macOS; совместимость строится по базовым ANSI/terminal-поведениям.

Узлы контракта и политики:
- ADR: `planning/adr/0002-pty-strategy.md`, `planning/adr/0003-render-fallback.md`
- Модульные интерфейсы: `planning/stack/v1.0.0-module-integration-contracts.md`

## OS-структура (логическое разделение)

```
crates/
  foundation/
    api/
    pty/
    window/
    clipboard/
    diagnostics/
  foundation-platform/
    linux/
    macos/
    windows/
  services/
  features/
    settings/
    render_cpu/
    render_gpu/
    shell_integration/
    diagnostics/
  ui/
  app/
```

## Ubuntu / macOS фокус v1.0.0
- Linux Ubuntu priority matrix: 22.04 LTS, 24.04 LTS, 25.10.
- macOS target: поддержка рендера + shell compatibility на этапе v1.0.0.
- Windows: отдельный слой уже в структуре, но включается как следующая фаза.

## Как обеспечиваем 3 главные качества
- Стабильность: строгие fallback chain и bounded retries.
- AI tools best practices: deterministic input processing + низкая шумность UX.
- Скорость: двойной рендер, dirty-rect обновления, минимальные аллокации в hot path и monitor-driven frame pacing.

## Риски и контроль
- GPU сбой: переключение в CPU mode + логирование причины + retry policy.
- Пропускные окна event loop: explicit redraw throttling.
- PTY утечки: ownership model + kill grace+hard fallback.


## 3) Прод production mapping: crate/feature split v1.0

### 3.1 Platform matrix by crate target

- Linux (v1.0): full foundation-platform/linux, runtime tested in `app` targets.
- macOS (v1.0): full foundation-platform/macos, same service stack.
- Windows (v1.0 skeleton): foundation-platform/windows provides compile-time adapter layer, not full user-facing parity.

### 3.2 Module mapping by layer

```text
app (cli/running profile)
  -> features
      -> services (session, render_mode, settings, diagnostics)
          -> core (grid/parser/state, bounded scrollback)
          -> foundation-platform adapters via foundation/api traits
```

### 3.3 Render fallback path

`auto` mode always executes:
- GPU warm-up and device/surface validation;
- on recoverable GPU failure: bounded retry (time/attempt window);
- on threshold breach: CPU mode switch and session keep-alive;
- emit `RenderModeTransition` event with explicit reason.
- on monitor change: re-resolve current monitor refresh-rate and re-apply render cadence without restart.

### 3.4 Required interface stubs for v1.0 implementation

- foundation-platform/linux/{pty,window}.rs
- foundation-platform/macos/{pty,window}.rs
- foundation-platform/windows/{pty,window}.rs (skeleton)
- features/render_cpu/{pipeline,stateful}.rs
- features/render_gpu/{pipeline,surface,sync,recovery}.rs
- services/session/pty_lifecycle.rs
- services/render_mode/controller.rs
- services/render_pacing/controller.rs
- services/settings/palette.rs
- ui/input_bridge.rs
