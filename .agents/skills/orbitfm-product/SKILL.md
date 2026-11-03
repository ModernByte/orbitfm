---
name: orbitfm-product
description: Продуктовые треки OrbitFM после 0.18.0-rc.1 — приоритеты Android, TUI, listening UX, portability, что не делать. Использовать при планировании релизов, оценке scope фич, спор «что делать раньше».
---

# OrbitFM Product

Ты помогаешь **выбирать scope и порядок** работ, опираясь на зафиксированные
product-документы и принятые ADR, а не на спонтанный wishlist.

## Источники истины

1. `docs/audit/OrbitFM-future-features.md` — приоритеты, волны, non-goals
2. `docs/audit/OrbitFM-tui-ratatui.md` — TUI-трек и его место в продукте
3. `docs/ROADMAP.md`, `docs/VERSIONMAP.md`, `docs/CHANGELOG.md`
4. Принятые ADR (ADR-0045 Android, ADR-0046 ритуалы, ADR-0047 TUI)
5. `orbitfm-core`

## Маршрутизация

- **Когда:** планирование релиза/волны, оценка фичи, спор о порядке работ,
  ответ «что делаем дальше», проверка scope на non-goals.
- **Обязательные спутники:** `orbitfm-core`, `orbitfm-docs`; при изменении
  контракта — `orbitfm-architect`. При переходе к реализации —
  профильный скилл (`orbitfm-android`, `orbitfm-tui`,
  `orbitfm-listening-ux`, `orbitfm-library-portability`, …).
- **Не подменяет:** архитектурные решения и реализацию. Product-скилл не
  разрешает код вне ROADMAP и не отменяет принятый ADR.
- **Правила:** `docs/AGENTS.md` §2.2.

## Статус треков (2026-11-03)

| Трек | Статус |
|------|--------|
| Android Companion | ADR-0045 принят, документация готова, кода нет; приоритет A |
| Sleep / queue / library export | база реализована в 0.18.0-rc.1; быстрый sleep — 0.19.0-rc.1 |
| Network profile / stats / URL-алиасы | профиль сети — desktop 0.19.0-rc.1 (ADR-0048); локальная статистика — desktop 0.19.0-rc.2 (ADR-0049), без экспорта и «карты года»; из приоритета B остались URL-алиасы и Android metered flag |
| Desktop tech debt | распил монстр-файлов и релизный контур закрыты в 0.17; хвосты по мере необходимости |
| TUI ratatui | ADR-0047 принят, T0 docs выполнен, код T1+ не начат; parallel track, не впереди Android v1 |
| Web Explorer, Telegram, Discord RPC, D-Bus | приоритет D, только отдельный ADR по спросу |

## Приоритеты (канон аудита)

| Приоритет | Трек | Скилл |
|-----------|------|--------|
| **A** | Android Companion | `orbitfm-android` |
| **B** | Sleep / queue / network profile / stats / aliases | `orbitfm-listening-ux` |
| **B** | Library export/import | `orbitfm-library-portability` |
| **C** | Desktop polish и техдолг | `orbitfm-architect`, `orbitfm-rust`, профильные |
| **D** | TUI ratatui (parallel track) | `orbitfm-tui` |
| **D** | Web Explorer, Telegram, Discord RPC, D-Bus… | отдельный ADR по спросу |

Если владелец не переопределил: **не** ставь TUI или новый шелл выше Android v1.

## Правила оценки фичи

1. Усиливает ли **слушание** (play, discover, return tomorrow)?
2. Требует ли **второго audio engine** или облака? (обычно нет)
3. Ломает ли IPC/library schema без migration path?
4. Раздувает ли файлы >1000 LOC без split?
5. Есть ли место в ROADMAP/VERSIONMAP и запись в CHANGELOG?
6. Не противоречит ли принятому ADR (Android v1, ритуалы, TUI, URL-политика)?

## Явно не делаем сейчас

- Смена GUI-стека desktop ради «красоты».
- Полный 3D-глобус на Android v1.
- Облачный аккаунт ModernByte для sync.
- Полноценный podcast/file music player.
- Новые bar-плагины «для галочки» при живых непатченных интеграциях.
- Пятый релизный артефакт TUI без решения владельца.

## Шаблоны ответа при планировании

- «Это волна 1/2/3 из future-features, потому что…»
- «Конфликтует с non-goals Android v1…»
- «Сначала interface/ADR, потом код».
- «Параллельно можно TUI spike, но релизный фокус — …»
- «Фича уже реализована в 0.18.0-rc.1; остался только …»

## Релизные артефакты

Desktop: AppImage / Flatpak / MSI / DMG (как в `packaging/`).
Android — отдельный AAB/APK и независимый semver.
TUI — source-only `cargo install` до отдельного решения о поставке.

## Чего нельзя

- Подменять ROADMAP устным «давайте ещё» без записи в docs.
- Смешивать store listing Android с обещанием desktop globe.
- Игнорировать Linux First для desktop-трека.
- Выдавать документацию TUI/Android за реализацию.
- Менять принятый ADR ради удобства текущей фичи.
