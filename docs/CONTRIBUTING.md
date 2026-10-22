# Руководство для участников (CONTRIBUTING)

**Проект:** OrbitFM  
**Организация:** ModernByte  

Спасибо, что хотите внести вклад. Этот документ описывает правила разработки, стиль кода и процесс работы.

## 1. Общие принципы

1. **Linux First** — всё сначала должно отлично работать на Linux (Wayland).
2. **Ничего не выдумываем** — любые технические решения проверяются по официальной документации.
3. **Документируем решения** — значимые архитектурные выборы фиксируются в ADR.
4. **Single-instance** — приложение всегда работает как один процесс.
5. **Безопасность и надёжность** — особенно в FFI (libmpv) и сетевом коде.

## 2. Стиль кода (Rust)

- **Edition:** 2024
- **Форматирование:** `rustfmt` (стандартные настройки)
- **Линтинг:** `clippy` с уровнем `pedantic` (где уместно) + `cargo deny`
- **Именование:**
  - Модули и файлы — `snake_case`
  - Типы — `PascalCase`
  - Функции и переменные — `snake_case`
  - Константы — `SCREAMING_SNAKE_CASE`
- **Ошибки:** используем `thiserror` + `anyhow` (где нужно)
- **Асинхронность:** `tokio`, предпочитаем `async fn` + structured concurrency
- **FFI:** весь небезопасный код изолируется в минимальные `unsafe`-блоки с комментариями «почему это безопасно»

### Обязательные проверки перед коммитом

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo deny check
```

## 3. Структура репозитория

```
orbitfm/
├── assets/
├── noctalia-plugin/
├── wm-configs/
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── audio/
│   ├── geo/
│   ├── render/
│   ├── services/
│   ├── media/
│   ├── ipc/
│   └── wm/
├── docs/
│   ├── architecture/
│   ├── adr/
│   ├── modules/
│   ├── interfaces/
│   ├── patterns/
│   └── templates/
├── .agents/skills/
├── .github/
│   ├── PULL_REQUEST_TEMPLATE.md
│   ├── COMMIT_CONVENTION.md
│   └── ISSUE_TEMPLATE/
├── Cargo.toml
└── README.md
```

## 4. Процесс работы с изменениями

1. Создаём issue по шаблону (`.github/ISSUE_TEMPLATE/`) или обсуждаем в Discussions.
2. Для архитектурных решений — сначала ADR (статус «Предложено»).
3. Ветка: `feature/...`, `fix/...`, `docs/...`, `adr/...`
4. Коммиты — по [`.github/COMMIT_CONVENTION.md`](../.github/COMMIT_CONVENTION.md) (Conventional Commits).
5. Pull Request — шаблон [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md).
6. Перед merge — [docs/templates/MERGE_CHECKLIST.md](templates/MERGE_CHECKLIST.md).
7. Статусы и labels — [docs/templates/PROJECT_STATUS.md](templates/PROJECT_STATUS.md).

## 5. Документация

- Любое новое публичное API — документация в коде (`///`)
- Значимые решения — ADR
- Структура кода — [docs/patterns/00-overview.md](patterns/00-overview.md)
- Изменения поведения — запись в CHANGELOG

## 6. AI-агенты

При работе с AI-агентами обязательно соблюдаются правила из `docs/AGENTS.md` и скиллы в `.agents/skills/`.

## 7. Контакты

- Организация: [ModernByte](https://github.com/ModernByte)
- Основной автор: code_warlord

---

Соблюдение этих правил помогает сохранять качество и предсказуемость проекта.
