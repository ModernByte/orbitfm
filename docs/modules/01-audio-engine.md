# Модуль: Audio Engine

**Путь в коде:** `src/audio/`  
**Ответственность:** Воспроизведение интернет-радиостанций, управление сессией libmpv, реконнект, агрегированные FFT-метрики для визуализатора.
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
    pub async fn set_spectrum_enabled(&mut self, enabled: bool) -> Result<(), AudioError>;
    pub fn spectrum_enabled(&self) -> bool;
    pub fn spectrum(&self) -> Option<SpectrumFrame>;
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumFrame {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub rolloff_hz: f32,
    pub flatness: f32,
    pub flux: f32,
    pub rms_db: f32,
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
| `mod.rs` | Публичные типы `PlaybackState`, `AudioEvent`, `AudioError`, `SpectrumFrame`; реэкспорт `AudioEngine` |
| `engine.rs` | Фасад движка: инициализация mpv, канал команд, `oneshot`-ответы, `Drop` |
| `worker.rs` | Выделенный поток: цикл команд и событий mpv, реконнект-политика, опрос FFT-метрик |

## 4. Внутреннее устройство

- `AudioEngine::new()` создаёт `Mpv` и запускает поток `orbitfm-audio`.
- Команды передаются через `tokio::sync::mpsc::UnboundedSender`; каждая несёт
  `oneshot::Sender<CommandResult>`.
- `play`, `stop`, `set_paused`, `set_volume`, `set_spectrum_enabled` ждут ответ с таймаутом 3 с.
- Состояние, громкость и последний FFT-кадр хранятся в `Shared` под `Mutex` (устойчиво к отравлению).
- События публикуются в `broadcast`-канал ёмкостью 64.
- Поток опрашивает `mpv_wait_event` каждые 50 мс и между опросами разбирает команды;
  при включённом визуализаторе с той же частотой читает `af-metadata`.

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

## 7. FFT-визуализатор

- PCM не перехватывается: к работающей сессии mpv по команде подключается фильтр
  `@spectrum:lavfi=[aspectralstats=measure=centroid+spread+flatness+flux+rolloff:win_size=2048,astats=metadata=1:reset=1]`.
- `aspectralstats` даёт FFT-метрики кадра, `astats` — RMS-уровень; оба прозрачны для звука.
- Поток `orbitfm-audio` читает строковые свойства `af-metadata/spectrum/...` раз в
  50 мс, пока состояние — `Playing` или `Buffering`, и складывает `SpectrumFrame`.
- Нефинитные значения отбрасываются; отрицательные и выходящие за диапазон —
  ограничиваются (`flatness` в `0.0..=1.0`, частоты неотрицательны).
- При выключении свойство `af` очищается, кадр сбрасывается. Новый `play`, `stop` и
  ошибки также очищают кадр.
- Если фильтр недоступен, mpv возвращает ошибку, команда отдаёт
  `AudioError::Spectrum`; воспроизведение не прерывается.
- Полосы спектра по бинам не поддерживаются; подробности и альтернативы — ADR-0010.

## 8. Ошибки

Все ошибки мапятся в `AudioError` (thiserror).  
`Init` означает, что модуль неработоспособен; Application Core считает это фатальной
ошибкой запуска. Остальные варианты возвращаются из конкретной команды.

## 9. Тестирование

- Unit-тесты: валидация URL (только http/https с хостом), диапазон громкости, backoff,
  исчерпание попыток, сброс трекера, разбор и ограничение FFT-метрик.
- Живой тест `plays_and_stops_live_stream` (`#[ignore]`, требует сеть и аудиоустройство):
  `ORBITFM_TEST_STREAM_URL=... cargo test --all-targets plays_and_stops_live_stream -- --ignored`
  проверяет play → pause → resume → stop.
- Живой тест `produces_fft_metrics_for_live_stream` (`#[ignore]`) включает визуализатор,
  ждёт до 20 с первый `SpectrumFrame`, проверяет конечность значений и выключает фильтр.

## 10. Текущие ограничения

- FFT доступен только как агрегированные метрики; полос спектра нет.
- Наличие `aspectralstats` зависит от сборки libavfilter (проверено на FFmpeg 12.1 / mpv 0.41).
- Метаданные ICY (название трека) не читаются — это задача будущих этапов.
- Поддержка Windows/macOS не проверялась.

## 11. Связанные ADR

- ADR-0003: Выбор аудио-движка
- ADR-0010: Экспериментальный FFT-визуализатор
