# rldyourterm bootstrap

В этом репозитории добавлен исходный код rldyourterm как сабмодуль в каталоге `rldyourterm`.

## 1) Инициализация субмодуля

```bash
# После клонирования этого репозитория:
git submodule update --init --recursive
```

## 2) Простой запуск локально

```bash
./scripts/rldyourterm
```

Скрипт запускает локальный source-экземпляр через `cargo run --release -- start` и использует локальный runtime для вашего переименованного терминала.

## 3) Что изменено прямо сейчас

- добавлен сабмодуль `rldyourterm` (upstream в `main`)
- добавлен локальный запускатель `./scripts/rldyourterm`
- git-исключения для x11-графики (`finl_unicode`, `xcb-imdkit`) переведены на форки в `rldyourmnd`

## 4) Ребрендинг статуса

- CLI-обвязка `rldyourterm-stable` и конфиг/документация уже переведены на новый бренд

## 5) Карта форков зависимостей

- `docs/operations/dependency-forks.md`
