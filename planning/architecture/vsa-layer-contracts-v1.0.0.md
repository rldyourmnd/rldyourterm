# VSA Layer Contracts v1.0.0 (Obligatory, implementation-synced)

## 1) Нормативная цель
Эти контракты — жёсткая граница для реализации: любая архитектурная правка должна быть совместима с ними или явно зарегистрирована как gap.

## 2) Слои и ответственность (crate ownership map)

### foundation/
- Отвечает за интеграцию со средой выполнения ОС и boundary error model.
- Реализация в текущем workspace:
  - `crates/foundation` — API traits/contracts/types/errors.
  - `crates/foundation-platform` — platform implementations (`pty`, `window`, `clipboard`).
- Текущий wiring status:
  - PTY adapter подключён в runtime.
  - Clipboard adapter подключён в runtime path app.
  - Window adapter подключён как primary runtime path в app window lifecycle (`G-010` closed).
- Не содержит доменной логики терминала.

### core/
- Отвечает за терминальную доменную модель:
  - grid/attrs/cursor/scroll/dirty-state,
  - parser/escape handling,
  - domain events/state contracts.
- Не зависит от `winit`/`wgpu`/OS API.

### services/
- Оркеструет:
  - lifecycle сессии и child process;
  - render modes (`cpu/gpu/auto`) и fallback transitions;
  - monitor-driven frame pacing и cadence re-sync;
  - recovery policy (bounded retry/degrade/controlled stop);
  - runtime decisions для UI/app.

### features/
- Модульные capabilities:
  - `settings` (command parser + transactional apply);
  - `render_cpu`;
  - `render_gpu`;
  - `shell_integration` (`fish/zsh`);
  - `diagnostics`.
- В GUI/TTY palette UI v1.0 используется shortcut-first action surface.
- Free-form command-line input inside palette UI не входит в текущий scope.

### ui/
- Отвечает за визуальное/runtime-поведение поверх `services`.
- Не владеет PTY lifecycle и не должен содержать platform-specific window/clipboard integrations.

### app/
- CLI и runtime bootstrap (`crates/app`).
- Window lifecycle в app runtime проходит через foundation window contracts (`WindowFactory/WindowControl`).
- Clipboard path в app уже проходит через foundation adapter.

## 3) Dependency rule (обязательная)
Зависимости должны течь внутрь:

- target flow: `app -> features -> services -> core`
- `foundation` подключается через контракты `foundation/api`.
- Прямой импорт feature-кода в `core` запрещён.

Если есть фактическое отклонение от target flow, оно должно быть явно зафиксировано в gap register с критериями закрытия.

## 4) Failure containment
- Любой failure на границе адаптера нормализуется в foundation/runtime error envelope + structured event.
- Сервисный слой обязан переводить failure в:
  1) retry (bounded),
  2) degrade mode,
  3) controlled stop с диагностикой без silent data-loss.

## 5) Version and stability guard
- v1.0.0 не допускает silent fallback без логирования.
- Любой `GPU -> CPU` переход фиксируется как `RenderModeTransition`.

## 6) Совместимость v1.0
- Приоритет: базовые ANSI/cursor/color/scroll/paste сценарии без crash path.
- Экзотические расширения допускаются с деградацией без падения сессии.
- Переход окна между 144Hz/60Hz должен приводить к cadence re-sync без session drop.
