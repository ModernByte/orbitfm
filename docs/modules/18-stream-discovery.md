# Модуль: Stream Discovery (`src/services/stream.rs`, `src/session/stream_import.rs`)

**Путь в коде:** `src/services/stream.rs`, `src/session/stream_import.rs`,
`src/ipc/protocol.rs` (`PlayUrl`), `integrations/browser/popup.js` (Play this page)  
**Ответственность:** довести введённый пользователем адрес страницы, плейлиста
или аудиофайла до воспроизводимого `http(s)`-URL, создать синтетическую
станцию и запустить её тем же путём, что радиостанцию.  
**Зависимости:** `reqwest` (ручные редиректы, `chunk()`), `url`, `tokio`
(таймауты и фоновая задача), `thiserror`; новых crate-зависимостей нет.  
**Решение:** ADR-0040 «Импорт внешних потоков»

---

## 1. Назначение и границы

Stream Discovery — это одна явная команда `orbitfm play-url <url>` и одна
кнопка «Play this page» в попапе. Автозапуска нет: ни открытие окна, ни
восстановление сессии, ни браузерный мост не запускают импорт сами.

| Живёт в модуле | Не живёт в модуле |
|---|---|
| Резолв URL, разбор m3u/pls/HTML, отказ HLS, лимиты и ошибки | Состояние эфира, очередь, громкость (`Session`) |
| Синтетическая станция, история, ответ клиенту | Видимая выдача поиска окна (`Session::companion`) |
| Скан активной вкладки в popup | Плагины панелей и трей |

`play-url` — единственная сетевая команда, которую принимает работает
экземпляр без участия GUI. Она не порождает второй процесс и не обходит
single-instance: при отсутствии приложения приходит `not-running`.

## 2. Состав пакета

| Файл | Ответственность |
|------|-----------------|
| `src/services/stream.rs` | `resolve`, `ResolvedStream`, `StreamError`, классификация, редиректы, m3u/pls/HTML, защита от циклов |
| `src/session/stream_import.rs` | `PendingStream`, `begin_play_url`, `poll_stream_import`, `finish_stream_import`, `start_play_url`, `stream_station`, `stream_id` |
| `src/ipc/protocol.rs` | `IpcCommand::PlayUrl { url }`, `MAX_PLAY_URL_BYTES`, строгий разбор `play-url` |
| `src/session/park.rs` | Ветка `Parked::StreamImport`: результат приходит при скрытом окне |
| `integrations/browser/popup.js` | Скан вкладки, список кандидатов, `play-url` по клику |

## 3. Резолвер

`resolve(input)` выполняется целиком под бюджетом `RESOLVE_BUDGET = 8 с`;
каждый HTTP-обмен — не дольше `REQUEST_TIMEOUT = 5 с`. Ответ IPC-сервера
клиенту — 10 с, клиентский таймаут — 12 с, поэтому резолвер обязан уложиться
в 8 с, а не «примерно в десять».

```text
play-url <url>
   │ validate: http(s), host, без userinfo, не .m3u8
   ▼
fetch_following (ручные редиректы ≤ 5, validate на каждом хоппе)
   │
   ├─ HLS (m3u8, vnd.apple.mpegurl, #EXT-X-, link rel=alternate) → ошибка
   ├─ playlist (.m3u/.pls/Content-Type): ≤ 20 записей, ≤ 2 уровня,
   │     первый http(s)-элемент, относительные через Url::join,
   │     цикл по visit-set → ошибка
   ├─ HTML (text/html, xhtml): og:audio → <audio src> → <source src>,
   │     title из og:title/<title>, ≤ 3 страницы в цепочке
   ├─ audio/*, application/ogg или аудио-расширение → готово
   └─ text/plain, octet-stream: тело ≤ 1 МиБ, затем m3u/pls/HTML или отказ
```

Классификация — по `Content-Type` и расширению, тело читается только когда
без него тип не определить. `read_bounded` проверяет `Content-Length` и
накапливает `chunk()` с жёстким потолком 1 МиБ. Принимаемые аудио-расширения:
`mp3`, `aac`, `ogg`, `oga`, `opus`, `flac`, `wav`, `m4a`, `m4b`, `webm`.

Плейлист даёт название из `#EXTINF` (`m3u`) или `TitleN` (`pls`). Первая
запись выбирается `first_playable`: не-`http(s)` строки пропускаются, а
HLS-ссылка отклоняет плейлист целиком, а не подменяется соседней записью.

HTML-скан не использует HTML-парсер: теги и атрибуты разбираются вручную,
сущности `&amp; &quot; &#39; &lt; &gt;` раскрываются. Ссылки приводятся к
абсолютным через `Url::join` и повторно проходят `validate`.

## 4. Синтетическая станция

После успешного резолва `start_play_url` собирает `Station`:

| Поле | Значение |
|------|----------|
| `stationuuid` | `url:` + 16 hex символов FNV-1a 64 от **исходного** адреса |
| `name` | `og:title`/`#EXTINF`/`TitleN`/`<title>`, иначе имя файла, иначе «Поток» |
| `url`, `url_resolved` | конечный воспроизводимый адрес |
| `homepage` | исходная страница или плейлист |
| `source` | `StationSource::Stream` |
| `favicon`, `tags`, `country`, `bitrate`, `codec` | пусто / `0` |

Станция проходит тот же `start_station`, что радио: очередь, история, MPRIS,
громкость и события `status` работают без веток. Click-трекинг Radio Browser
пропускается: `url:`-идентификатор не существует на сервере. Повторный импорт
того же адреса даёт тот же `stationuuid`, поэтому избранное и история
ссылаются на один поток между сессиями.

## 5. Отложенный ответ и жизненный цикл

Резолв не выполняется в `execute`: `begin_play_url` создаёт `PendingStream`,
хранит исходный `IpcRequest` и запускает `stream::resolve` в tokio. Результат
возвращается через `mpsc`, будит GUI и забирается:

- `tick()` — когда окно открыто;
- ветка `Parked::StreamImport` в `park()` — когда окно скрыто.

Клиент получает `ok` только после `start_station`; ошибка резолвера приходит
строкой `error <причина>`. Второй `play-url` до завершения первого получает
`error импорт уже выполняется` — два параллельных резолва не нужны и путали
бы ответы по времени.

## 6. Безопасность

- **Только явное действие.** Команду запускает пользователь: CLI или клик в
  попапе. Ни окно, ни трей, ни восстановление сессии её не вызывают.
- **Только `http(s)`.** Схема и отсутствие userinfo проверяются на входе и на
  каждом редиректе: `file://`, `ftp://` и `http://user:pass@…` отклоняются.
- **LAN разрешён.** Это осознанное исключение из SSRF-политики обложек
  (`services/ssrf.rs`): адрес вводит сам пользователь, а сервер не получает
  новых прав. Приватные диапазоны не блокируются.
- **HLS отклоняется** по расширению, MIME, телу `#EXT-X-` и
  `link rel=alternate`: mpv-конфигурация OrbitFM его не поддерживает.
- **Лимиты.** Тело — 1 МиБ, редиректы — 5, уровни плейлистов — 2, записи —
  20, HTML-страницы — 3, URL команды — 2048 байт.
- **Без прокси и хранения.** Резолвер не кэширует тела, не пишет файлы и не
  переиспользует соединения после ответа.

## 7. Ошибки

`StreamError` показывается пользователю на русском в попапе и CLI:

| Вариант | Сообщение |
|---------|-----------|
| `InvalidUrl` | некорректный URL (включая userinfo и пустой host) |
| `UnsupportedScheme` | поддерживаются только http:// и https:// |
| `Hls` | HLS-потоки (.m3u8) не поддерживаются |
| `TooLarge` | страница или плейлист больше 1 МиБ |
| `NotAudio` | аудиопоток не найден |
| `Http(u16)` | HTTP-статус |
| `Network` | сетевая ошибка |
| `Timeout` | превышен таймаут импорта |
| `EmptyPlaylist` | плейлист не содержит воспроизводимых ссылок |
| `RedirectLimit` | слишком много редиректов |
| `PlaylistDepth` | слишком много уровней плейлиста |
| `PlaylistCycle` | цикл в плейлисте или странице |

## 8. Попап

«Play this page» делает скан только по явному клику и только активной
вкладки: `og:audio`, `<audio src>`, `<source src>`, URL вкладки. Относительные
адреса приводятся через `new URL`, остаются только `http(s)`, дубликаты
убираются, кандидатов ≤ 20. Без разрешения `scripting` остаётся URL вкладки;
автозапуска нет — выбранный кандидат уходит в `play-url`, ошибка показывается
в панели.

## 9. Тесты

`src/services/stream.rs`:

- `resolves_relative_m3u_entry_and_title`, `resolves_pls_file_and_title`;
- `resolves_html_og_audio_and_title`, `resolves_audio_tag_and_document_title`;
- `rejects_hls_by_extension_mime_and_body`,
  `rejects_hls_alternate_link_on_page`;
- `follows_redirects_and_rejects_loops`,
  `accepts_octet_stream_with_audio_extension_without_body`;
- `rejects_oversized_pages`,
  `rejects_empty_playlist_and_non_audio_page`,
  `rejects_playlist_cycles_and_depth`;
- `validation_refuses_non_http_userinfo_and_hls`,
  `attribute_scanner_handles_quotes_and_entities`.

`src/session/stream_import.rs`: `stream_id_is_stable_and_distinct`,
`station_uses_title_homepage_and_stream_source`,
`station_falls_back_to_file_name_then_stream`,
`file_name_ignores_query_and_extension`.

Тесты резолвера поднимают локальный TCP-сервер и проверяют реальные
HTTP-обмены: редиректы, циклы, размер тела, относительные ссылки и отказ HLS.

## 10. Живая приёмка

2026-10-31, Linux x86_64, 0.16.0-rc.2, `python3 -m http.server` на
`127.0.0.1:8765`:

- `orbitfm play-url` для `playlist.m3u`, `playlist.pls` и `page.html` с
  `og:audio` довёл каждый адрес до станции: синтетический
  `url:<fnv1a64>`, `source: stream`, имя из `#EXTINF`/`og:title`/имени файла,
  запись в истории и избранном, запуск из истории через `play-library` без
  сети, отображение в статусе и MPRIS;
- отклонены с внятной ошибкой: HLS (`.m3u8`), `file://`, URL с userinfo,
  HTTP 404 и тело страницы больше 1 МиБ;
- медленный сервер на `127.0.0.1:8766` (заголовки через 6 с): одиночный
  импорт завершился «превышен таймаут импорта» (5 с на запрос), а второй
  параллельный запуск получил `error импорт уже выполняется`;
- попап: настоящий action-попап с `activeTab` отсканировал локальную страницу,
  показал кандидатов `page.html` и `tone.wav`, импорт выбранной страницы дал
  тот же `url:a6ea…` и `source: stream`.

Ограничение доказательности: редиректы, относительные ссылки, циклы
плейлистов и глубина вложенности проверены тестами резолвера на локальном
TCP-сервере, а не отдельным живым прогоном.

## 11. Ссылки

- ADR-0040 — импорт внешних потоков
- ADR-0038 — браузерное расширение и native messaging host
- `docs/interfaces/05-ipc.md` — команда `play-url` и ответы
- `docs/modules/17-browser-companion.md` — попап и скан вкладки
- `docs/wiki/12-Браузерное-расширение.md` — пользовательская инструкция
