# Модуль: Audio Engine

**Путь в коде:** `src/audio/`  
**Ответственность:** Воспроизведение интернет-радиостанций, управление сессией libmpv, реконнект, получение PCM (для FFT в перспективе).  
**Зависимости:** `libmpv2` 6.0 (безопасная обёртка над C API libmpv), `tokio` (каналы и таймауты), `libc` (локаль)

---

## 1. Назначение

Единственный владелец аудиосессии в процессе OrbitFM.  
Все команды play / pause / stop / volume / reconnect проходят только через этот модуль.
Модуль не знает о станциях, Radio Browser и MPRIS — он работает с готовым URL.

## 2. Публичный интерфейс (контракт)

Полное описание — `docs/interfaces/01-audio.md`. Кратко:

```rust
pub struct AudioEngine { /* ... */ }

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError>;
    pub async fn play(&mut self, url: &str) -> Result<(), AudioError>;
    pub async fn stop(&mut self) -> Result<(), AudioError>;
    pub async fn set_paused(&mut self, paused: bool) -> Result<(), AudioError>;
    pub async fn set_volume(&mut self, volume: f64) -> Result<(), AudioError>;
    pub fn state(&self) -> PlaybackState;
    pub fn volume(&self) -> f64;
    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Reconnecting,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEvent {
    Started { url: String },
    Stopped,
    Buffering,
    Reconnecting { attempt: u32 },
    Error { message: String },
    EndOfFile,
}
```

## 3. Состав модуля

| Файл | Назначение |
|------|------------|
| `mod.rs` | Публичные типы `PlaybackState`, `AudioEvent`, `AudioError`; реэкспорт `AudioEngine` |
| `engine.rs` | Фасад движка: инициализация mpv, канал команд, `oneshot`-ответы, `Drop` |
| `worker.rs` | Выделенный поток: цикл команд и событий mpv, реконнект-политика |

## 4. Внутреннее устройство

- `AudioEngine::new()` создаёт `Mpv` и запускает поток `orbitfm-audio`.
- Команды передаются через `tokio::sync::mpsc::UnboundedSender`; каждая несёт
  `oneshot::Sender<CommandResult>`.
- `play`, `stop`, `set_paused`, `set_volume` ждут ответ с таймаутом 3 с.
- Состояние и громкость хранятся в `Shared` под `Mutex` (устойчиво к отравлению).
- События публикуются в `broadcast`-канал ёмкостью 64.
- Поток опрашивает `mpv_wait_event` каждые 50 мс и между опросами разбирает команды.

Опции mpv при инициализации:

| Опция | Значение | Причина |
|-------|----------|---------|
| `config` | `no` | не читать пользовательский конфиг mpv |
| `terminal` | `no` | не писать в терминал |
| `idle` | `yes` | процесс живёт без файла; `stop` не завершает mpv |
| `vid` | `no` | только аудио |
| `network-timeout` | `10` | ограниченное ожидание сети |

Дополнительно: `LC_NUMERIC` принудительно устанавливается в `C` (требование libmpv),
громкость задаётся как `volume = 0.7 * 100`, отслеживается свойство `paused-for-cache`.

## 5. Состояния и события

| Переход | Условие | Событие |
|---------|---------|---------|
| `Stopped` → `Buffering` | команда `play` | `Buffering` |
| `Buffering` → `Playing` / `Paused` | `FileLoaded` (учитывается свойство `pause`) | `Started { url }` |
| `Playing` → `Buffering` | `paused-for-cache = true` | `Buffering` |
| `Buffering` → `Playing` | `paused-for-cache = false` | `Started { url }` |
| `Playing`/`Buffering` → `Paused` | `set_paused(true)` | — |
| `Paused` → `Playing` | `set_paused(false)` | `Started { url }` |
| любой активный → `Reconnecting` | обрыв или `EndFile(Error/Eof)` | `Reconnecting { attempt }` |
| `Reconnecting` → `Playing` | успешный `FileLoaded` | `Started { url }` |
| `Reconnecting` → `Error` | попытки исчерпаны | `Error { message }` |
| любой → `Stopped` | команда `stop` | `Stopped` |

Причины `EndFile::Stop`, `EndFile::Quit` и `EndFile::Redirect` реконнект не запускают.

## 6. Политика реконнекта

- Максимум 5 попыток.
- Задержки: 1, 2, 4, 8, 16 с (экспоненциальный backoff с потолком 16 с).
- Повторный `loadfile` выполняется из потока; ошибка команды тоже расходует попытку.
- Пока ожидается повтор, новые обрывы не создают дополнительных попыток.
- `FileLoaded`, `play`, `stop` сбрасывают счётчик.

## 7. Ошибки

Все ошибки мапятся в `AudioError` (thiserror).  
`Init` означает, что модуль неработоспособен; Application Core считает это фатальной
ошибкой запуска. Остальные варианты возвращаются из конкретной команды.

## 8. Тестирование

- Unit-тесты: валидация URL (только http/https с хостом), диапазон громкости, backoff,
  исчерпание попыток, сброс трекера.
- Живой тест `plays_and_stops_live_stream` (`#[ignore]`, требует сеть и аудиоустройство):
  `ORBITFM_TEST_STREAM_URL=... cargo test --all-targets plays_and_stops_live_stream -- --ignored`
  проверяет play → pause → resume → stop.

## 9. Текущие ограничения

- PCM для FFT ещё не отдаётся; появится вместе с экспериментальным визуализатором.
- Метаданные ICY (название трека) не читаются — это задача будущих этапов.
- Поддержка Windows/macOS не проверялась.

## 10. Связанные ADR

- ADR-0003: Выбор аудио-движка
