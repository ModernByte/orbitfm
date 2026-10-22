# Сообщения коммитов (OrbitFM)

Формат: [Conventional Commits](https://www.conventionalcommits.org/), язык — русский или английский (главное — ясность).

## Структура

```
<type>(<scope>): <краткое описание>

[тело — почему, не только что]

[футер: Breaking Change, Refs]
```

## Types

| Type | Когда |
|------|--------|
| `feat` | Новая возможность |
| `fix` | Исправление бага |
| `docs` | Документация |
| `refactor` | Рефакторинг без смены поведения |
| `perf` | Производительность / память |
| `test` | Тесты |
| `chore` | CI, зависимости, мета |
| `adr` | Новый или обновлённый ADR |
| `style` | Форматирование (редко; предпочитайте fmt в CI) |

## Scope (примеры)

`audio`, `radio`, `geo`, `gui`, `ipc`, `media`, `app`, `noctalia`, `docs`, `build`

## Примеры

```
feat(ipc): команда volume и ответ status

fix(audio): реконнект после обрыва HLS-потока

docs(adr): ADR-0009 каталог паттернов

perf(radio): кэш станций в Box<[Station]>

adr: принять playwire как media backend
```

## Правила

- Одна логическая идея — один коммит (или серия связанных).
- Не смешивать `feat` и массовый `docs`/`fmt` без необходимости.
- Breaking changes — в теле/футере: `BREAKING CHANGE: ...`
- Ссылка на issue: `Refs: #123` или `Closes: #123`
