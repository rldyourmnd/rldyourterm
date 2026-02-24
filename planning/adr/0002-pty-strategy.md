# ADR-0002: PTY Strategy for v1.0.0

## Status
Accepted

## Context
Нужна максимальная стабильность длинных сессий в Linux/macOS.
Риск: нестабильная PTY-обвязка = падение shell или потеря состояния.

## Decision
- v1.0.0 использует `portable-pty` как единственную кроссплатформенную PTY абстракцию.
- Своя PTY-реализация как core не пишется в v1.0.
- Реализуем адаптерный слой:
  - `foundation/pty` — создание PTY, resize, spawn, read/write, kill.
  - `services/session` — retry policy и корректное завершение.
- Важные ограничения:
  - не держать writer `portable-pty` более одного раза;
  - всегда закрывать writer при смене сессии.
- Ошибка PTY не приводит к падению app:
  - переход в `degraded` state;
  - завершение с информированием и корректным stop shell-сессии.

## Rationale
- `portable-pty` уже поддерживает runtime abstraction (Unix/Windows) и предоставляет базовые API для:
  - `openpty`, `spawn_command`, `resize`, `try_clone_reader`, `take_writer`, `Child::try_wait/kill`.
- Это минимизирует платформенные риски в v1.0.

## Consequences
- Shell интеграция делается как адаптер на этом слое.
- Никаких собственных PTY-демонов или winpty-экспериментов в v1.0.
- Все PTY-ошибки нормализуются в service events (`PtyError`, `PtyRecoverable`, `PtyFatal`).

## References
- Context7 `portable-pty` API: `PtySystem`, `MasterPty`, `Child` traits.  
  Source: `https://docs.rs/portable-pty/` (via Context7 `/websites/rs_portable-pty`)
