# v1.0.0 Definition of Done

## Scope
Это финальный контрольный список для закрытия вехи v1.0.0.

## Preconditions
- `planning/v1.0.0-development-blueprint.md` актуален.
- `planning/discovery/v1.0.0-answer-lock.md` закрыт.
- Все ADR статусы подтверждены и не конфликтуют.

## Hard gates
- `planning/quality/v1.0.0-quality-gates.md` — Alpha/Beta/RC выполнены.
- `planning/quality/v1.0.0-acceptance-matrix.md` полностью заполнен и закрыт.
- `planning/operations/v1.0.0-manual-test-plan.md` выполнен на целевых платформах.
- `planning/operations/v1.0.0-release-pack.md` завершен до sign-off.
- `planning/risk/v1.0.0-risk-matrix.md` не содержит открытых High-рисков без mitigation.

## Release consistency
- Версия `1.0.0` отражена во всех релевантных документах.
- Архитектурные и стековые решения не конфликтуют между файлами.
- Нет CI-зависимости для выпуска; все проверки ручные и документированы.

## Final acceptance
- Подтверждено по каждому из пунктов: стабильность, AI-практики, скорость.
- Режимы `cpu/gpu/auto` работают в базовых сценариях.
- Fallback происходит автоматически, без падения сессии.
