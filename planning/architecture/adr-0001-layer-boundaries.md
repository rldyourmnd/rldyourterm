# ADR-0001: VSA Layer Boundaries for v1.0.0

## Status
Accepted

## Context
Нужно избежать “монолитного” терминала, где UI и ядро сливаются с OS и рендером.

## Decision
1. Фиксируем шесть слоев:
   - `foundation`, `core`, `services`, `features`, `ui`, `app`.
2. `core` не имеет зависимостей на `winit`, `wgpu`, платформенный API.
3. `foundation` общается с `core` только через adapter trait- boundaries.
4. Все внешние интеграции инкапсулируются и подменяемы в `services`/`features` через контрактные API.

## Rationale
- Ускоряет стабилизацию: при падении рендер-модуля перезапуск/переход в fallback не разрушает сессионную модель.
- Облегчает диагностику: failure всегда локализован в границе слоя.
- Позволяет запускать CPU path без GPU code-path без изменения core.

## Consequences
- Дополнительная буферизация через trait-interfaces.
- Строгое правило code ownership по слоям и архитектурному ревью.
- Прямые импорты между несоседними слоями в v1.0 запрещены.

## Related
- `planning/architecture/vsa-layer-contracts-v1.0.0.md`
- `planning/v1.0.0-development-blueprint.md`
- `AGENTS.md`
