# Integration Architecture (v1.0.0 Planning, 2026-02-24)

## Ключевой ответ на вопрос
Да, мы будем интегрироваться, но только с OS- и runtime-примититвами:
- `portable-pty` — создание и управление PTY, spawn shell, resize, read/write.
- `winit` — окно, цикл событий, клавиатура/мышь, resize/window lifecycle.
- `wgpu` — GPU рендер backend (cross-platform).
- Наши собственные `core/text` + `render.cpu` + `render.gpu`.

Никаких зависимостей от чужого terminal-движка как архитектурного ядра.

## Что подключаем сейчас (v1.0.0)
1. PTY Layer (`foundation/pty`)
- `portable-pty`
- Задачи: spawn shell, master/slave IO, resize, kill/safe shutdown.

2. Event/Window Layer (`foundation/window`)
- `winit`
- Задачи: create window, scale DPI, keyboard/mouse input, redraw scheduling.

3. Render Core (`features/render-core`)
- CPU path: internal raster/compose path без GPU зависимостей.
- GPU path: `wgpu` + internal text cache/quad pipeline.

4. Text Pipeline (`features/text`)
- Начать с минимального внутреннего шейпинга и рендера для ASCII/UTF-8 baseline.
- Модульно открыть путь для `cosmic-text`/`glyphon` при росте потребности в шейпинге/ligatures.

5. Settings UX (`features/settings`)
- In-terminal command palette + runtime config actions.

6. Shell Integration (`features/shell-integration`)
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
    src/platform/
      linux.rs    // current target
      macos.rs    // current target
      windows.rs  // adapter skeleton for roadmap
  services/
  features/
    settings/
    render_cpu/
    render_gpu/
    shell/
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
- Скорость: двойной рендер, dirty-rect обновления, минимальные аллокации в hot path.

## Риски и контроль
- GPU сбой: переключение в CPU mode + логирование причины + retry policy.
- Пропускные окна event loop: explicit redraw throttling.
- PTY утечки: ownership model + kill grace+hard fallback.
