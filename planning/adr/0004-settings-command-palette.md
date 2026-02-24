# ADR-0004: In-terminal Settings as Primary UX

## Status
Accepted

## Context
v1.0 ориентирован на продуктового пользователя и AI workflow. Конфигурации через редактирование dotfiles в v1.0 не удовлетворяют UX.

## Decision
1. `Settings UI` реализуется только через in-terminal command palette (`Ctrl/Cmd + Shift + P`).
2. Изменение настроек только через typed command model:
   - `theme set cuberpunk`
   - `shell fish|zsh|auto`
   - `mode cpu|gpu|auto`
   - `shell auto-init on|off`
   - `performance fps-target <hz>`
3. Изменения конфигурации:
   - валидируются на schema;
   - применяются live;
   - журналируются с `settings.apply` событием.
4. Любой ошибочный вход не ломает сессию; показываем диагностическое сообщение и rollback в `previous valid state`.

## Rationale
- Упрощает эксплуатацию и снижает риск “bad config file”.
- Поддерживает быструю адаптацию во время работы терминала.
- Совместимо с long-running AI сценариями без перезапуска.

## Consequences
- Надо реализовать parser команд palette и историю изменений.
- Конфиг всё равно persistence хранится в typed local profile (не user-edit-first).

## Related
- `planning/settings/settings_palette.md`
- `planning/quality/v1.0.0-quality-gates.md`
