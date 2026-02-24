# v1.0.0 Quality Targets

## Глобальная цель
Приоритеты: стабильность -> AI-совместимость -> скорость.

## Целевые измеримые показатели

### Стабильность
- Controlled crash-free ratio: долгие AI сессии должны проходить без падения сессии.
- GPU failure recovery: любое recoverable GPU failure → переход в CPU за предсказуемое время.
- no session drop при падениях PTY/рендера/resize storms в сценариях ручной и AI-нагрузки.

### AI Compatibility
- Предсказуемый путь ввода/вывода: стабильный prompt round-trip.
- Корректная работа copy/paste и scrollback без потери состояния курсора/цвета.
- Runtime-настройки должны применяться без перезапуска.

### Производительность
- CPU mode: низкая активность простоя при idle (без busy-loop в hot path).
- GPU mode: ускорение рендера без влияния на стабильность сессии.
- Auto mode: при доступном GPU — использование GPU, при fault — мягкий fallback на CPU.
- Frame pacing: частота рендера берётся из текущего монитора и корректно пересинхронизируется при переносе окна между экранами (например, 144Hz -> 60Hz).

### Ограничения
- Базовый scrollback cap: 50_000 строк.
- No full multiplexer и no external config-only UX в v1.0 baseline.
