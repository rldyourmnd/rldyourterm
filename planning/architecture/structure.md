# VSA Architecture Notes (Implemented Baseline)

## Архитектурная нота v1.0.0 (закрытый baseline)

- Ядро и адаптеры изолированы через trait boundaries.
- Платформенные интеграции живут в `foundation`/`infrastructure`.
- Слои `features` изолированы и версионируются с независимыми интерфейсами.
- Эта нота служит справочным дополнением к `planning/architecture/adr-0001-layer-boundaries.md` и `planning/architecture/vsa-layer-contracts-v1.0.0.md`.
