# Модуль: Библиотека станций

**Путь в коде:** `src/library/mod.rs`  
**Ответственность:** Хранение избранных станций и истории прослушивания, локальная персистентность, экспорт и импорт JSON схемы 1.  
**Зависимости:** serde, serde_json, thiserror, `services::{Station, publishable}`, `storage::write_atomic`

---

## 1. Назначение

Единственная точка работы с пользовательской библиотекой станций.  
Модуль предоставляет снимки станций, переживающие перезапуск процесса, и не знает
о GUI, аудио, IPC и сети. Решение — ADR-0013.

## 2. Состав модуля

| Файл | Назначение |
|------|------------|
| `mod.rs` | `StationLibrary`, `LibraryError`, `ImportSummary`, `default_path`, нормализация, экспорт/импорт схемы 1 |

Модуль хранит два списка: `favorites` (избранное) и `history` (история).
`Station` берётся из `services` как канонический доменный тип; копия полей
не создаётся.

## 3. Данные и инварианты

- Избранное — без жёсткого лимита, порядок добавления.
- История — не более `HISTORY_LIMIT = 20` станций, от новых к старым.
- Уникальность в обоих списках — по `stationuuid`.
- Хранится полный снимок `Station`, поэтому запуск не требует сети.
- Пустая история и пустое избранное — валидное состояние.

## 4. Персистентность

```
$XDG_DATA_HOME/orbitfm/library.json
$HOME/.local/share/orbitfm/library.json   (если XDG_DATA_HOME не задан/относителен)
```

- Формат — JSON `{ "favorites": [...], "history": [...] }`, читаемый вручную.
- Запись атомарная и durable (`src/storage.rs`): `library.json.tmp` → `sync_all`
  → `rename` → fsync каталога; каталог создаётся при необходимости. Файл
  переживает не только падение процесса, но и отключение питания.
- Запись синхронная, вызывается только на действие пользователя (переключение
  избранного, старт станции, очистка истории). Фоновой записи нет.
- Отсутствующий файл — пустая библиотека, не ошибка.
- Повреждённый JSON — `LibraryError::Parse`; Application Core создаёт
  `StationLibrary::ephemeral()`, которая не пишет файл, поэтому повреждённые данные
  не теряются.
- Ошибки записи не прерывают воспроизведение; Application Core показывает их
  тостом.

## 5. Нормализация

При загрузке `normalize`:

1. удаляет дубликаты по `stationuuid` в избранном и истории (остаётся первое
   вхождение);
2. обрезает историю до `HISTORY_LIMIT`.

Это защищает от ручных правок файла и старых версий с другим лимитом.

## 6. Экспорт и импорт

Перенос библиотеки между desktop и Android Companion — JSON схемы 1
(`docs/interfaces/15-library-export.md`, ADR-0046). Контракт общий, поэтому
полевые имена и правила очистки совпадают с Android-клиентом.

| Метод | Поведение |
|-------|-----------|
| `export_json` | Pretty-JSON `{schema, app, exported_at, favorites, history}`; строки очищены от управляющих символов и обрезаны по длине |
| `export_to` | Пишет `export_json` атомарно (`storage::write_atomic`), затем ставит права `0600`; возвращает число избранных |
| `import_from` | Проверяет абсолютность/относительность пути, потолок файла 2 МиБ, читает и передаёт в `import_bytes` |
| `import_bytes` | Проверяет `schema == 1`, `app` (`""` или `orbitfm`), суммарный потолок 5000 станций, санитизирует каждую станцию и сливает по UUID |
| `ImportSummary` | `favorites_added`, `history_added`, `skipped` — что именно дало слияние |

Правила импорта:

- станции добавляются в конец избранного и истории; существующие по
  `stationuuid` не дублируются, но их содержимое не перезаписывается;
- невалидные станции пропускаются (`skipped`), а не отменяют импорт целиком;
  принимаются только публичные `http(s)`-адреса без userinfo (`publishable`,
  ADR-0025);
- история после слияния нормализуется и обрезается до `HISTORY_LIMIT`;
- запись на диск выполняется только если что-то добавилось; эфемерная
  библиотека молча остаётся в памяти;
- чужая схема, чужое `app` или превышение лимитов — `LibraryError::Parse`
  без изменения текущей библиотеки.

## 7. Связь с Application Core

- `main` загружает `StationLibrary::load_default()` до `run_gui`; при ошибке
  логирует предупреждение и использует `ephemeral()`.
- `start_station` вызывает `record_played` после успешного `audio.play`; это
  касается строки результатов, избранного, истории, маркера и IPC `play`.
  Возобновление из MPRIS историю не меняет.
- Переключение звезды вызывает `toggle_favorite`; ошибки логируются и
  показываются тостом.
- Вкладки «Результаты», «Избранное (N)», «История (N)» и кнопка «Очистить историю»
  живут в `ui/station_panel.rs`; модуль не содержит UI-типов.

## 8. Ограничения

- Нет временных меток прослушивания и сортировки избранного.
- Нет синхронизации между процессами: единственность обеспечивает single-instance.
- `Next`/`Previous` MPRIS не используют историю: она не является очередью
  воспроизведения (ADR-0013).
- Импорт не перезаписывает существующие станции и не удаляет отсутствующие:
  это слияние, а не зеркало.
- Сетевой синхронизации нет: обмен идёт только файлом через
  `library-export`/`library-import` (ADR-0046).
- Экспорт включает локальные `stream`-станции из `play-url`; импорт проверяет
  публичность URL и такие записи пропускает (`skipped`). Перенос библиотеки с
  локальными потоками поэтому не полный, в том числе при повторном импорте.

## 9. Тестирование

Unit-тесты в `src/library/mod.rs`:

- `default_path_prefers_absolute_xdg_data_home`
- `default_path_falls_back_to_home_for_relative_xdg`
- `missing_file_loads_empty_library`
- `roundtrip_persists_favorites_and_history`
- `toggle_favorite_adds_and_removes_station`
- `history_deduplicates_moves_to_front_and_caps`
- `corrupt_file_returns_parse_error`
- `load_normalizes_duplicates_and_history_limit`
- `ephemeral_library_keeps_changes_without_file`
- `export_import_roundtrip_merges_and_deduplicates`
- `export_import_skips_local_streams_by_public_url_policy`
- `import_rejects_unknown_schema_and_foreign_app`
- `import_rejects_oversized_payload_and_station_count`
- `import_skips_hostile_stations_and_sanitizes_accepted_ones`
- `export_writes_private_file_and_import_reads_it_back`

Временные каталоги — через dev-зависимость `tempfile`; реальный `$HOME` не
используется.

## 10. Связанные документы

- ADR-0013: Избранное и история станций
- ADR-0005: Single-instance и IPC
- ADR-0046: «Ритуалы слушания» — экспорт/импорт библиотеки и его схема
- `docs/interfaces/08-library.md`
- `docs/interfaces/15-library-export.md`
- `docs/modules/07-app-core.md`
