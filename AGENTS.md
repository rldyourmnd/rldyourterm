# AGENTS.md - Project Policy for rldyourterm (Codex Runtime)

## Scope
- Applies to repository: `/home/rldyourmnd/Desktop/projects/nddev_projects/awesome-terminal-for-ai`.
- Governs architecture, quality, and execution discipline for all sessions.

## 1) Product Priorities (hard priority order)
1. `СТАБИЛЬНОСТЬ`
   - Сохранение сессий при сбоях рендеринга.
   - Корректный failover: GPU -> CPU без завершения процесса терминала.
   - Детект, retry/backoff и наблюдаемость для всех критических границ.
2. `BEST PRACTICES FOR AI TOOLS`
   - Поведение, заточенное под CLI-потоки CodeX, OpenCode, Claude Code, Gemini CLI.
   - Предсказуемая производительность ввода/вывода в автоматических сессиях.
   - Минимум шума и низкая дестабилизация в рабочем процессе AI.
3. `СКОРОСТЬ`
   - Минимальная латентность (input -> frame).
   - Два режима рендера: `cpu` и `gpu` + `auto`.
   - Контролируемое потребление RAM/CPU.

## 1.1) Product Persona and Compatibility Intent
- Пользователь v1.0: визуальный продуктовый владелец и AI CLI power-user с длительными сессиями.
- Shell policy: дефолтно `fish + starship`; допускается fallback `zsh`.
- Bash не является целевым shell для v1.0 на Linux/macOS; совместимость базовых ANSI/terminal-сценариев обязательна.
- Критичные сценарии AI: prompt latency, paste latency, корректный scrollback, отсутствие цветовых/курсорных артефактов.

## 1.2 OS Scope and Delivery Stage
- v1.0.0: Linux (Ubuntu 22.04 LTS, 24.04 LTS, 25.10, поддержка Debian-пакетной экосистемы), macOS.
- Windows: отдельный архитектурный слой и адаптер в `foundation`, включение полной функциональности после v1.0.
- Будущее расширение ОС не влияет на функциональные гейты v1.0.

## 2) VSA Архитектурная модель (обязательная)
- `foundation/`: адаптеры ОС и интеграции (PTY, window/event loop, clipboard, env hooks).
- `core/`: доменная модель терминала (grid/state/parser/events), без зависимостей от OS API.
- `services/`: оркестрация сессий, режимов рендера, retry/fallback и контролируемого завершения.
- `features/`: модульные возможности (render.cpu, render.gpu, settings.ui, shell-integration, diagnostics).
- `ui/`: визуальное поведение на основе сервисных контрактов.
- `app/`: CLI и сборка бинарей.

## 2.1) Правила зависимостей
- Зависимости только внутрь:
  - `app -> features -> services -> core`
  - `foundation` подключается через explicit adapter trait-port interfaces в `foundation/api`.
- Прямой импорт `core` в `foundation` запрещён.
- Любые внешние crates в  v1.0 подключаются только через адаптеры.

## 3) Hard Constraints v1.0 (не изменяются без ADR)
- Поддержка трёх render режимов: `cpu`, `gpu`, `auto`.
- Автопереключение `auto`: GPU first, затем bounded retry, затем CPU.
- In-terminal настройки обязательны как primary UX (`Ctrl/Cmd + Shift + P`).
- Ограничение сложности: базовый терминал один tab/окно в v1.0.
- Debug mode for diagnostics: по умолчанию выключен, включается runtime-командой и фиксируется в событии журнала.
- Схема метрик: structured logs + event-id + trace correlation для сбоев GPU/PTY/window.
- Лимит строки состояния: целевой максимум 50_000 строк scrollback в v1.0.

## 4) Stability/Performance Governance
- GPU failure не падает на core loop.
- `GPU -> CPU` fallback всегда логируется и приводит к уведомлению пользователя.
- Любое падение, restart, fallback — наблюдаемое событие через `trace` и `diagnostics`.

## 5) Работа с документами и релизом
- Перед переходом к коду фиксируется документационный lock:
  - `planning/discovery/v1.0.0-answer-lock.md`
  - `planning/v1.0.0-development-blueprint.md`
  - `planning/architecture/adr-0001-layer-boundaries.md`
  - `planning/adr/0002-pty-strategy.md`
  - `planning/adr/0003-render-fallback.md`
  - `planning/adr/0004-settings-command-palette.md`
  - `planning/stack/v1.0.0-module-integration-contracts.md`
  - `planning/quality/v1.0.0-quality-gates.md`
  - `planning/risk/v1.0.0-risk-matrix.md`
- Ветвление и финальные этапы без CI: только manual test pack и ручной QA.
- Версия v1.0.0 должна быть консистентно зафиксирована во всех `metrics/version/1.0.0/*.md`.

## 6) Non-goals v1.0.0 (запрещено по умолчанию)
- Multiplexer-режим в v1.0.0.
- Полноценные windows/мульти-оконные сценарии в v1.0.0.
- Расширенные визуальные эффекты (blur/shadow/gradients) в v1.0.0.
- Внешние файлы-конфиги как первичный путь настройки.
