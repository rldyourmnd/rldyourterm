# VSA Architecture Notes (Prod-Ready Baseline, v1.0 target)

## 1) Core principle

Проект строится как production-ready, но сжатый к v1.0, каркас с четкими границами и отказоустойчивым fallback:

- foundation — platform ports.
- core — терминальная модель без платформенных зависимостей.
- services — orchestration и отказоустойчивость.
- features — capabilities поверх services.
- ui — рендер/interaction слой.
- app — CLI, запуск, lifecycle и сборка.

## 2) Target crate graph

```text
crates/
  app/
  ui/
  features/
    render_cpu/
    render_gpu/
    settings/
    shell_integration/
    diagnostics/
  services/
  core/
  foundation/
    api/
    pty/
    window/
    clipboard/
    logging/
  foundation-platform/
    linux/
    macos/
    windows/
```

Dependency direction must be strict:
`app -> features -> services -> core`

`foundation` подключается только через API-трейт-границы из `foundation/api`.

## 3) Cross-platform split (prod-ready)

```text
foundation/
  api/
    session.rs
    window.rs
    clipboard.rs
    telemetry.rs
  pty/
    mod.rs
    adapter.rs
    error.rs
  window/
    mod.rs
    event_translation.rs
    state.rs
  clipboard/
    mod.rs
```

```text
foundation-platform/
  linux/
    pty/
    window/
    env/
  macos/
    pty/
    window/
    env/
  windows/
    pty/
    window/
    env/
```

OS-platform crates are adapter implementations only; they should be optional behind cargo target features.

## 4) Data and event contracts (high-level)

- `GridModel` in core stores screen matrix, style, cursor and dirty regions.
- `InputEvent`, `PtyEvent`, `WindowEvent`, `RenderModeEvent`, `SettingsEvent` are domain events in services.
- `WindowEvent` должен содержать сигнал о смене monitor timing, чтобы services могли пересчитать cadence без hardcoded fps.
- `DiagnosticsEvent` is canonical telemetry envelope with stable `event_id`, severity and correlation ids.
- `SettingsPatch` applies by transaction with rollback on validation failure.

## 5) Stability and recovery boundaries

- Не допускается падение core-состояния из-за ошибок adapt-слоёв.
- Ошибка адаптера должна нормализоваться в `DomainError` и обработаться в services.
- Для каждой failure-path выполняется одна из стратегий: retry, degrade, controlled stop.

## 6) Почему это production-ready

- Явные границы ownership и resource lifecycle для writer/reader/child.
- Отдельный render decision service, чтобы не смешивать UI и сессию.
- Отдельная monitor-driven cadence логика в services, чтобы не смешивать UI, window API и scheduler policy.
- Предсказуемый `gpu -> cpu` переход и явный журнал событий.
- План развития OS-спец. адаптеров не ломает core-сигнатуры.
