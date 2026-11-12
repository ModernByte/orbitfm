# OrbitFM — аудит 0.22.0-rc.1

| Поле | Значение |
|------|----------|
| **Дата** | 2026-11-12 |
| **Версия** | `0.22.0-rc.1` |
| **Коммит** | `6dac9d8` |
| **Объём** | ~60 717 LOC Rust, 126 `.rs`, 60 ADR |
| **Источник** | https://github.com/ModernByte/orbitfm |

Аудиты: рендер, многопоточность, сеть, состояние/устойчивость, безопасность. Опирается на дерево исходников, не на бенчмарк с железа ревьюера.

---

## 0. Резюме

| Область | Оценка (1–5) | Главный вывод |
|---------|--------------|---------------|
| 2.1 Рендер | **4.0** | Дискретная GPU + worker текстур — сильно; риск — лишние `request_repaint` и тяжёлые кадры motion/markers |
| 2.2 Потоки | **4.0** | Audio/tray/texture/IPC разнесены; следить за `block_on` и границей UI↔worker |
| 2.3 Сеть | **4.2** | Таймауты, ручные redirect, SSRF tiered; можно — единый client pool и backoff RB |
| 2.4 Устойчивость | **3.9** | Snapshot/reconnect/sleep есть; panic boundary не везде; крупные модули session |
| 2.5 Безопасность | **4.3** | SSRF/artwork/stream зрелые; IPC local-trust; supply chain через deny — держать |

**P0 сейчас:** нет критичного аналога «не стартует на Mac» в этом срезе кода.  
**P1:** бюджет кадра / repaint policy; изоляция паник worker; RB rate/backoff.  
**P2:** метрики, уменьшение monster-файлов session/render.

---

## 2.1. Рендер и отрисовка

### Что уже хорошо

- **egui-wgpu CallbackTrait** для глобуса — 3D не через CPU soft-render.
- **ADR-0053 / 0058:** выбор адаптера с предпочтением **дискретной GPU**, `AdapterReport` в лог — снижает риск «рисуем на iGPU при наличии NVIDIA/AMD».
- **Фоновая загрузка текстур** в `globe.rs`: decode/mip/upload в worker; `Device`/`Queue` как multithread handles — UI-кадр не декодирует JPEG Земли синхронно.
- Heatmap / markers / motion вынесены в отдельные модули (`heatmap`, `markers`, `motion`, `camera`).
- Часть UI уже использует `request_repaint_after` (callout, spectrum, favicon), а не вечный continuous mode.

### Риски и узкие места

| Риск | Почему важно |
|------|----------------|
| Частые `request_repaint()` / короткий `request_repaint_after` | Держит event-loop и GPU в тонусе без необходимости (idle CPU, ноутбуки) |
| `motion.rs` ~1.5k LOC + camera ~1.4k | Сложно гарантировать «дешёвый тик» при reduce_motion / паузе |
| Markers + heatmap + orbit ring + spectrum одновременно | Fill-rate и CPU project markers на слабом iGPU |
| Синхронная работа на UI-thread вне callback | Любой тяжёлый geo/search на кадре = dropped frame |

### Что улучшить

1. **Политика repaint (P1)**  
   - Явные режимы: `Playing+Visible` / `Idle` / `reduce_motion`.  
   - Idle: не чаще 1–5 FPS или только по input/IPC.  
   - Playing: spectrum/globe pulse — целевой FPS с потолком (например 30), не «каждый event».

2. **Бюджет кадра (P1)**  
   - Tracing spans: `globe_paint`, `motion_tick`, `markers_project`, `egui_full`.  
   - Раз в rc — зафиксировать p95 на интегральной GPU и на discrete (как уже частично делали для 1050 Ti в CHANGELOG).

3. **Markers LOD (P2)**  
   - При сильном zoom-out: агрегировать/сэмплировать точки, не проецировать все 500+ с full cost.  
   - Отсекать back-face / за горизонтом раньше дорогого path.

4. **Текстуры (P2)**  
   - Проверить максимальный размер earth texture и mip chain на Intel UHD.  
   - Не пересоздавать bind group, если текстура не менялась.

5. **Спейсинг обновлений spectrum (P2)**  
   - Выровнять частоту FFT/overlay с audio worker period; не дублировать repaint из нескольких мест за один тик.

### Не делать

- Переписывать стек с wgpu на другой API ради микрооптимизаций.  
- Включать continuous rendering «чтобы было гладко» без политики idle.

---

## 2.2. Многопоточность и параллелизм

### Что уже хорошо

- **Audio worker** (`audio/worker.rs`, `engine.rs`) — выделенный поток, UI не зовёт libmpv напрямую.
- **Tray (ksni)** — отдельный `std::thread` (обоснованно: blocking API).
- **Texture upload worker** в render/globe.  
- **Tokio** для IPC client, native host, сетевых fetch (artwork, stream resolve).  
- Windows **SMTC host window** — свой поток.

### Риски

| Риск | Деталь |
|------|--------|
| `block_on` на UI/park путях | Документирован в ssrf (ярус 1 без DNS на main); любой новый DNS на UI — регресс |
| Паника в worker | Без `catch_unwind`/restart policy процесс может убить весь app |
| Каналы UI↔audio↔session | Разрастание `session/mod.rs` (~1.3k) — сложнее инварианты «кто владеет state» |
| Двойной runtime | Смешение std::thread + tokio — ок, но нужен явный список «кто где живёт» в architecture doc |

### Что улучшить

1. **Panic isolation (P1)**  
   - Audio worker и texture worker: `catch_unwind` на витке loop + безопасный restart или controlled shutdown с notice в UI.  
   - Не глотать ошибку молча — `tracing::error` + user-visible degraded mode.

2. **Контракт каналов (P1)**  
   - Короткая таблица в `docs/architecture`: команда → очередь → поток → ответ.  
   - Backpressure: что если UI шлёт play чаще, чем worker успевает.

3. **Избегать тяжёлого `block_on` (P1)**  
   - Audit `rg block_on` в PR checklist для UI path.

4. **Параллелизм сети (P2)**  
   - Ограниченный semaphore на concurrent favicon/artwork (уже могут быть in-flight limits — проверить единый ли лимит на весь app).

5. **Join/shutdown order (P2)**  
   - При `quit`: порядок stop audio → flush library → join workers с таймаутом (не висеть вечно).

---

## 2.3. Сетевые запросы

### Что уже хорошо

- **reqwest + rustls**, явные timeout.  
- **Artwork / stream**: ручные redirect, лимиты тела, DNS timeout, проверка public IP (SSRF).  
- **Radio Browser**: discovery, кэш, client с политикой.  
- **Snapshot** offline (ADR-0043) — деградация без сети.  
- Stream resolve budget (`RESOLVE_BUDGET`), max redirects/playlist levels.

### Риски

| Риск | Деталь |
|------|--------|
| Много `Client::builder()` ad-hoc | Нет единого connection pool / HTTP/2 reuse между подсистемами |
| RB под нагрузкой | Нет явного global rate limit / exponential backoff при 429/5xx во всех путях |
| Повторные world fetch | Лишняя батарея/трафик на metered (network profile — если не везде включён) |
| Icy/metadata | Парсинг сторонних потоков — нужен жёсткий лимит буфера (частично есть MAX_*) |

### Что улучшить

1. **Shared Reqwest client (P1)**  
   - Один (или два: «catalog» и «untrusted-artwork») `Client` с pool, timeout defaults, user-agent OrbitFM.

2. **Retry policy (P1)**  
   - Идемпотентные GET к RB: 1–2 retry с jitter на timeout/5xx; **не** retry слепо на 404.  
   - Уважать `Retry-After` если появляется.

3. **Единый in-flight limiter (P1)**  
   - Artwork + favicon + stream probe не должны открывать десятки соединений одновременно.

4. **Metered / economy path (P2)**  
   - Связать session economy с network profile: реже snapshot refresh, меньше prefetch artwork.

5. **Observability (P2)**  
   - Счётчики: rb_ok, rb_fail, artwork_ssrf_reject, stream_resolve_ms (tracing или debug status).

---

## 2.4. Состояние, восстановление, ошибки, отказ устойчивости

### Что уже хорошо

- Settings / library persistence, atomic writes (паттерн storage).  
- Offline **stations snapshot**.  
- Reconnect / adaptive bitrate path (ADR-0043).  
- Sleep timer, park/window modes — session разделена на подмодули.  
- Часть decode path с `catch_unwind` (изображения).  
- IPC single-instance — один процесс, предсказуемый lifecycle.

### Риски

| Риск | Деталь |
|------|--------|
| `unwrap`/`expect` в hot paths | По `rg -c` высокие счётчики в settings/ipc/library/audio — часть в тестах, часть нет; нужен audit «production path only» |
| Session god-module | `session/mod.rs` ~1310 LOC — сложнее гарантировать восстановление после partial failure |
| Нет единого supervisor | Падение одного подсистемного потока ≠ управляемый degraded mode |
| Коррупция JSON | Нужны schema version + ignore/rebuild (snapshot уже близко; library/settings — проверить) |

### Что улучшить

1. **Error budget UX (P1)**  
   - Любая ошибка сети/play → notice/toast с действием (retry / lighter stream / offline), не только log.

2. **Worker supervisor (P1)**  
   - Audio: при неожиданном exit worker — попытка один раз пересоздать engine или stop + сообщение, не silent dead controls.

3. **Снижение unwrap в non-test (P1)**  
   - `expect` только на инвариантах «type system guarantees»; иначе `map_err` + notice.

4. **Recovery checklist (P2)**  
   - Документ: cold start без сети; cold start с битым snapshot; kill -9 во время save; disk full.  
   - Автотест на битый JSON library/settings.

5. **Partial UI failure (P2)**  
   - Глобус не поднялся (no wgpu) → список станций + transport всё ещё работают (уже частично в AdapterReport path — закрепить тест/поведение).

---

## 2.5. Безопасность

### Что уже хорошо

- **`services/ssrf.rs`**: scheme, userinfo, single-label, localhost, private IP ranges, classify reasons.  
- Artwork/stream: redirect re-check, size limits, content-type allowlist patterns.  
- macOS dylib audit (после 0.20): нет `/tmp` Python path.  
- `cargo-deny` в packaging scripts.  
- IPC — local socket trust model (ожидаемо для desktop).

### Риски и пробелы

| Тема | Рекомендация |
|------|----------------|
| IPC без auth | Документировать threat model: любой local user = full control play/quit. Mode `0600` на sock. |
| HTTP artwork default | По возможности HTTPS-first; http только с теми же SSRF checks (уже) |
| Log injection | Не писать сырые URL/метаданные станций в shared logs без санитизации длины |
| Dependency CVEs | Регулярный `cargo deny` / audit в CI (не только ручной скрипт) |
| Native messaging host | Строгая валидация длины сообщений; не eval |
| Desktop deep links (future) | Не регистрировать схемы без валидации uuid/url |

### Что улучшить

1. **SECURITY.md (P1)** — threat model: local IPC, network SSRF, no remote admin.  
2. **CI cargo-deny на каждый PR (P1)** если ещё не mandatory green.  
3. **Fuzz/robustness (P2)** — парсеры playlist (m3u/pls) и JSON станций из RB.  
4. **Socket permissions assert (P2)** в тестах Linux.  
5. **Release artifacts** — продолжать SHA256SUMS; SignPath/Apple по отдельному треку.

---

## 3. Приоритетный бэклог после аудита

| Приоритет | Работа |
|-----------|--------|
| P1 | Repaint policy + frame budget instrumentation |
| P1 | Panic/restart policy для audio (и texture) worker |
| P1 | Shared HTTP client + RB retry/backoff |
| P1 | SECURITY.md + IPC trust explicit |
| P2 | Markers LOD / idle FPS cap |
| P2 | session/mod.rs и motion.rs — дальнейший split |
| P2 | Corrupt config recovery tests |
| P2 | Playlist parser hardening tests |

---

## 4. Связь с релизом 0.22.0-rc.1

MINOR про дискретную GPU и геометрию жестов — в правильную сторону для **2.1**. Этот аудит не откатывает решения 0.22; предлагает **следующий слой**: не только «какая GPU», но «как часто и насколько дорого кадр», плюс устойчивость worker/сеть.

---

## 5. История

| Дата | Изменение |
|------|-----------|
| 2026-11-12 | Аудит по дереву `0.22.0-rc.1` / `6dac9d8` |
