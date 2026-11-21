# План: каналы релизов и автообновление (ADR-0067)

**Организация:** ModernByte  
**Дата:** 2026-11-21  
**Статус:** запущен; ADR-0067 принят  
**Скиллы:** orbitfm-core, orbitfm-architect, orbitfm-ipc, orbitfm-rust,
orbitfm-docs; orbitfm-integration (уведомления/трей); orbitfm-tui (поверхность).

## Контекст

Обновление ручное, все релизы — предрелизы. Нужны канал `stable`/`dev`,
проверка по таймеру, уведомление и автоустановка AppImage. Дизайн — ADR-0067.

## Волны и задачи

### U1 — ядро модуля (сделано)
- [x] **UPD-1a** `src/update/`: `Channel`, `Release`, `ReleaseAsset`,
      `UpdateState`, сравнение по `semver` 1.0 (транзитивно в графе).
- [x] **UPD-1b** `src/update/github.rs`: `UpdateSource` (трейт) +
      `GithubSource` (reqwest, rustls), фильтр канала, выбор максимума,
      выбор ассета платформы; allowlist хостов, обязательный `User-Agent`.

### U2 — команда и состояние
- [x] **UPD-2a** CLI `update` / `update --check` / `update --check --json` /
      `update --channel <stable|dev>`; cold-start без GUI. **Сделано.**
- [ ] **UPD-2b** `update channel <…>` (сохранение) и `update install`;
      сейчас возвращают честную ошибку «ещё не реализовано».
- [ ] **UPD-3** настройки `update_channel`, `update_check`, `update_auto_install`.
- [ ] **UPD-4** аддитивное поле `update` в `status --json` (без сети).

### U3 — планировщик
- [ ] **UPD-5** tokio-задача в `Session`: задержка на старте, интервал по
      `update_check`, уважение `network_profile` (offline-first — не проверять,
      metered — без загрузки), `Notice` при смене `latest`, кэш результата.

### U4 — установка
- [ ] **UPD-6** AppImage: скачать ассет и `SHA256SUMS`, сверить SHA-256,
      временный файл + `chmod 0755` + атомарный `rename`; отказ без суммы.
- [ ] **UPD-7** MSI/DMG: только уведомление + URL (ADR-0052).

### U5 — поверхность и тесты
- [ ] **UPD-8** TUI: строка «update available» в статусе, клавиша `U` — проверка.
- [ ] **UPD-9** тесты: фикстуры Releases, сравнение версий, фильтр канала,
      выбор ассета, allowlist, сверка SHA-256; fake `UpdateSource`.
- [ ] **UPD-10** docs: interface 05, module (новый `docs/modules/21-update.md`),
      CHANGELOG/ROADMAP/VERSIONMAP, `docs/tui/README.md`.

## Критические файлы

Новые: `src/update/{mod,github,install,scheduler}.rs` ·
`docs/modules/21-update.md` · `docs/interfaces/19-update.md`.  
Правки: `src/settings/mod.rs` · `src/ipc/protocol.rs` · `src/session/mod.rs` ·
`src/main.rs` · `src/native_host.rs` (запрет `update install`) · `Cargo.toml`
(`semver`) · `docs/ROADMAP.md`, `CHANGELOG.md`.

## Решения владельца

1. Default-канал — `stable`; `dev` — opt-in.
2. Автоустановка default `false`; интервал default `daily`.
3. Сеть проверки уважает `network_profile`.

## Чего не делаем

Свой сервер обновлений; отдельный репозиторий на канал; подпись артефактов
(отдельный ADR); автоустановка MSI/DMG; root-демон; `git pull`.
