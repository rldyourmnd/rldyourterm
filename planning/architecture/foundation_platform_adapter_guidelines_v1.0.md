# Foundation Platform Adapter Guidelines v1.0

## Цель

i) Защитить domain-слои от платформенной нестабильности.
ii) Обеспечить одинаковую семантику поведения PTY/окна/clipboard/диагностики на Linux/macOS и скелетно на Windows.

## 1) OS-specific hard rules

### Linux
- Реализовать `foundation-platform/linux` полностью (без feature flags, production path in v1.0).
- Привязка ошибок: EIO/EOF/EOF-on-write должны быть recoverable, если возможно, с retry.
- Обязателен refresh-rate probe текущего монитора для monitor-driven frame pacing.

### macOS
- Реализовать `foundation-platform/macos` полностью.
- Учитывать scale-factor изменения и redraw semantics winit.
- Обязателен refresh-rate probe текущего монитора для monitor-driven frame pacing.

### Windows
- Реализовать `foundation-platform/windows` как compile-valid skeleton с explicit limitation notes.
- Вынести runtime-critical paths в conditional readiness checks или совместимые compatibility-заглушки (`runtime placeholders`) с диагностическими событиями.

## 2) Common adapter invariants

1. No panic passthrough: все ошибки в `RuntimeError`.
2. Non-blocking on normal path: `request_redraw`, input polling не должен блокировать render loop.
3. Strict ownership:
   - один writer на сессию,
   - один event sink per window,
   - один диагностика-сенк на runtime.
4. Deterministic close:
   - window close -> stop flag -> session drain -> child kill (grace timeout) -> cleanup.

## 3) PTY adapter lifecycle

1. init adapter
2. spawn child
3. stream pump
4. resize
5. shutdown:
   - graceful close
   - wait timeout
   - hard kill

## 4) Window adapter lifecycle

1. create
2. poll/init callbacks
3. redraw scheduling
4. scale handling
5. monitor-change detection (`Moved`/`ScaleFactorChanged`) + refresh-rate update
6. focus/close
7. shutdown

## 5) Diagnostics mapping

- Каждый adapter обязан emit минимум:
  - init/ready,
  - resize,
  - display refresh changed (если окно сменило монитор/частоту),
  - runtime error,
  - shutdown.

## 6) Как не сделать плохо

- Не дергать `core` из adapter напрямую.
- Не менять размеры/viewport напрямую без прохождения services.
- Не игнорировать `recoverable` флаги ошибок в `RuntimeError`.
