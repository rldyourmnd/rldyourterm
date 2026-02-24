# v1.0.0 Definition of Done

## Обязательное состояние для ручного релиза v1.0.0

### Документы и фиксация
- `AGENTS.md` и `planning/v1.0.0-development-blueprint.md` содержат цели и приоритеты v1.0.0.
- ADR и roadmap подтверждены.
- `planning/quality/v1.0.0-acceptance-matrix.md`, `planning/quality/v1.0.0-quality-gates.md` закрыты вручную.
- `planning/operations/v1.0.0-manual-test-plan.md` и `planning/operations/v1.0.0-release-pack.md` заполнены и подписаны.
- `metrics/version/1.0.0/*.md` содержат текущие решения, цели и DOD.

### Runtime требования (после реализации)
- Запускается один терминалное окно (single-window baseline).
- Поддержка режимов `cpu`, `gpu`, `auto`.
- Реализован контролируемый fallback `gpu -> cpu` с notification и диагностикой.
- In-terminal settings palette работает как primary-путь изменения runtime-конфига.
- Fish+Starship и zsh fallback стабильно воспроизводимы.

### Проверки качества
- Сквозная ручная валидация по Linux 22.04/24.04/25.10 и macOS.
- Проверены сценарии long-run (>=10 минут), burst output, resize storm, перенос окна между мониторами с разным refresh-rate, prompt/copy/paste, scrollback cap.
- Нет критичных blockers из risk matrix v1.0.0.
