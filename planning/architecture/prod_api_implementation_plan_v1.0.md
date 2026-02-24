# Prod API Implementation Plan v1.0 (foundation/api first)

## 1) Сначала создаем контракты

- `planning/architecture/foundation_api_contracts_v1.0.md`
- `planning/architecture/foundation_platform_adapter_guidelines_v1.0.md`

## 2) Затем инфраструктурный слой

- Реализовать `foundation/api` модули и типы ошибок.
- Поднять traits без импорта внешних runtime-контрактов в core/services.

## 3) Затем foundation-platform stubs

- Linux/macOS: PTY + window + clipboard + diagnostics adapter.
- Windows: skeleton stubs с явным статусом parity.

## 4) services integration

- Сервисы используют только `foundation/api` traits.
- Любая ошибка приводит к `DomainEvent` + policy handler (retry/degrade/stop).
- monitor-driven cadence решается в services через данные `foundation/api::window`, без numeric hardcode в features/ui.

## 5) features/ui wiring

- render_cpu/render_gpu consume только contracts и state snapshot, а не raw OS API.
- settings palette работает через service transaction API.

## 6) Тест-реестр

1. unit: contract invariants (writer, resize, close).
2. integration: mock adapters -> core lifecycle smoke.
3. e2e manual: `cpu`, `gpu`, `auto`, palette commands, fallback smoke, monitor transfer 144Hz<->60Hz.

## 7) Definition of Done for API contracts

- API docs + traits + error model compile and documented.
- Zero direct foundation usage in non-foundation crates.
- At least one implementation adapter for Linux/macOS passes smoke.
