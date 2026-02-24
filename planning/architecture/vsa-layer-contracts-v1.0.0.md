# VSA Layer Contracts v1.0.0 (Obligatory)

## 1) Нормативная цель
Эти контракты — жёсткая граница для реализации: каждая новая часть кода должна ссылаться на них.

## 2) Слои и ответственность

### foundation/
- Отвечает только за интеграцию со средой выполнения ОС.
- Содержит:
  - PTY адаптеры.
  - оконный слой (создание окна/события ввода/фокуса/Resize).
  - clipboard + system helpers.
  - профили логирования и runtime env.
- Не содержит доменной логики терминала.

### core/
- Отвечает за:
  - состояние терминальной модели (grid, атрибуты, скролл, dirty flags);
  - разбор escape/ANSI-последовательностей в своей модели;
  - контрактный интерфейс для feature и services.
- Не зависит от winit/wgpu/OS API.

### services/
- Оркеструет:
  - жизненный цикл сессии и child process;
  - режимы рендера (`cpu/gpu/auto`) и переключения;
  - recovery policy (retry/backoff/fallback);
  - глобальные события в runtime.

### features/
- Модульные capabilities:
  - settings (in-terminal palette);
  - render.cpu;
  - render.gpu;
  - shell integration (`fish/zsh`);
  - diagnostics (`metrics`, `logs`, traces).
- Каждый feature имеет только trait-интерфейс по public API и внутреннюю реализацию.

### ui/
- Отвечает только за визуальное поведение поверх `services` и `features`.
- Не знает ОС и не владеет PTY lifecycle.

### app/
- CLI и сборка бинарей.
- Нужен только для конфигурации рантайма и сборки сценариев запуска.

## 3) Dependency Rule (обязательная)
Зависимости только внутрь:
- `app -> features -> services -> core`
- `foundation` подключается через адаптеры (interfaces в `foundation/api`) и может использ. платформенные зависимости.
- Прямой импорт feature-кода в core запрещен.

## 4) Failure Containment
- Любой failure на границе адаптера превращается в `DomainError` + structured event.
- Сервисный слой обязан переводить failure в:
  1) retry (bounded),
  2) degrade mode,
  3) или controlled stop с диагностикой без data-loss.

## 5) Version and Stability Guard
- v1.0.0 не допускает silent fallback без логирования.
- Любой `GPU -> CPU` переход фиксируется как event (`RenderModeTransition`).

## 6) Совместимость v1.0
- Приоритет: корректная работа базовых ANSI/cursor/color/scroll/paste сценариев.
- Экзотические/редкие расширения допускаются с деградацией без падения сессии.
