# Модуль: обновления (update)

**Путь в коде:** `src/update/`  
**Ответственность:** проверка и установка обновлений OrbitFM по двум каналам
(`stable`/`dev`) из GitHub Releases; планировщик фоновой проверки; безопасная
установка AppImage.  
**Решение:** ADR-0067.

---

## 1. Состав

```text
src/update/
├── mod.rs        # Channel, Release, ReleaseAsset, UpdateState, UpdateError,
│                 # pick_release, format_utc_now
├── github.rs     # UpdateSource (трейт), GithubSource, parse_releases, allowlist хостов
├── cli.rs        # `orbitfm update [--check] [--json] [--channel <stable|dev>]`
├── scheduler.rs  # UpdateScheduler: «когда проверять» (частота, профиль сети)
└── install.rs    # (UPD-6) скачивание, SHA-256, атомарная замена AppImage
```

Канал `stable` — релизы без флага `prerelease` (`vX.Y.Z`); `dev` —
предрелизы (`vX.Y.Z-rc.N`). Сравнение версий — `semver` 1.0 (уже в графе
транзитивно).

## 2. Модель

- `Release { tag, version, channel, name, notes, published_at, html_url, assets }`;
- `ReleaseAsset { name, url, size }`;
- `Release::platform_asset()` — AppImage/`*.msi`/`*.dmg` по ОС и архитектуре;
- `Release::checksums_url()` — ассет `SHA256SUMS`;
- `UpdateState { channel, current, latest, checked_at }` + `available()`.

## 3. Обнаружение

`GET https://api.github.com/repos/ModernByte/orbitfm/releases?per_page=30` с
`User-Agent: OrbitFM/1.0 (ModernByte; <OS>; Rust)` и
`X-GitHub-Api-Version: 2022-11-28`. Черновики и не-semver теги отбрасываются;
ассеты с недопустимым хостом — ошибка (`UpdateError::Host`). Хосты — только
`api.github.com`, `github.com`, `objects.githubusercontent.com`, только HTTPS.

## 4. Команда `update`

Cold-start: сеть проверяется в процессе CLI, GUI и IPC не поднимаются.

| Команда | Поведение |
|---------|-----------|
| `update` / `update --check` | человекочитаемая проверка |
| `update --check --json` | снимок схемы 1 |
| `update --channel <stable\|dev>` | проверка выбранного канала |
| `update channel <…>` | сохранить канал в `settings.json` (UPD-3) |
| `update install` | скачать и установить (UPD-6) |

`--json` содержит `schema, channel, current, latest, available, checked_at,
release_url, published_at, notes, checksums, asset{name,size,url,sha256}`.

## 5. Планировщик (сделано)

`UpdateScheduler` (чистое решение, время инъектируется): первая проверка через
60 с, далее интервал по
`settings.update_check` (`off|daily|weekly`); уважает `network_profile`
(`offline-first` — не проверять, `metered` — только проверка без загрузки);
проверка запускается в `Session::tick` (окно) и `Session::park` (простой,
источник в `wait_on`); результат кэшируется и попадает в аддитивное поле
`update` `status --json`. Отдельного `Notice` нет — поверхность статус/TUI.

## 6. Установка (UPD-6)

Только Linux AppImage и только после сверки SHA-256 из `SHA256SUMS`:
скачать во временный файл рядом, `chmod 0755`, атомарный `rename` поверх
`~/.local/bin/orbitfm`. MSI/DMG — уведомление и URL (ADR-0052). Root не нужен,
shell не используется.
