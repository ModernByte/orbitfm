# Модуль: Audio Engine

**Путь в коде:** `src/audio/`  
**Ответственность:** Воспроизведение интернет-радиостанций, управление сессией libmpv, реконнект, агрегированные FFT-метрики и полосы Bass/Mid/High для визуализатора.
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
pub struct BandLevels {
    pub bass_db: f32,
    pub mid_db: f32,
    pub high_db: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumFrame {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub rolloff_hz: f32,
    pub flatness: f32,
    pub flux: f32,
    pub rms_db: f32,
    pub bands: Option<BandLevels>,
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
| `mod.rs` | Публичные типы `PlaybackState`, `AudioEvent`, `AudioError`, `SpectrumFrame`, `BandLevels`; реэкспорт `AudioEngine` |
| `engine.rs` | Фасад движка: инициализация mpv, канал команд, `oneshot`-ответы, `Drop` |
| `worker.rs` | Выделенный поток: цикл команд и событий mpv, реконнект-политика, выбор графа анализа, опрос FFT-метрик и полос |

## 4. Внутреннее устройство

- `AudioEngine::new()` создаёт `Mpv` и запускает поток `orbitfm-audio`.
- Команды передаются через `tokio::sync::mpsc::UnboundedSender`; каждая несёт
  `oneshot::Sender<CommandResult>`.
- `play`, `stop`, `set_paused`, `set_volume`, `set_spectrum_enabled` ждут ответ с таймаутом 3 с.
- Состояние, громкость и последний FFT-кадр хранятся в `Shared` под `Mutex` (устойчиво к отравлению).
- События публикуются в `broadcast`-канал ёмкостью 64.
- `spectrum_enabled` по умолчанию `true`; воркер при старте сразу ставит
  `af`-фильтры, поэтому визуализатор работает без ручного включения. Чип
  «Visualizer» может выключить его до конца сессии.
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

## 7. Метрики визуализатора

- PCM не перехватывается: к работающей сессии mpv подключаются два размеченных
  lavfi-фильтра (ADR-0027):
  - `@level` — `aspectralstats=measure=centroid+spread+flatness+flux+rolloff:win_size=2048,astats=metadata=1:reset=1`;
  - `@spectrum` — копия сигнала через `asplit`, `highpass=f=20`,
    `acrossover=split=150 2000:order=8th`, `amerge`, `astats` по каналам и `pan`,
    возвращающий основной сигнал.
- Фильтры ставятся при старте воркера (`spectrum_enabled = true`) и снимаются
  только командой `set_spectrum_enabled(false)`.
- `aspectralstats` даёт FFT-метрики кадра, `astats` в `@level` — общий RMS;
  `astats` в `@spectrum` — RMS восьми каналов (основной сигнал и три полосы L/R).
  Все фильтры прозрачны для звука.
- Раскладка `audio-params/channel-count` выбирает стерео- или моно-граф. При
  `FileLoaded` свойство может быть `nil`, поэтому воркер повторяет чтение до 8 раз
  с интервалом 250 мс; неизвестная или нестандартная раскладка оставляет только
  `@level`.
- Поток `orbitfm-audio` читает строковые свойства `af-metadata/level/...` и
  `af-metadata/spectrum/...` раз в 50 мс, пока состояние — `Playing` или
  `Buffering`, и складывает `SpectrumFrame`. При отсутствии `@spectrum` читаются
  только шесть ключей `@level`.
- Нижняя граница полос — `BAND_FLOOR_DB = -90.0`; уровень полосы объединяет L/R
  средним по мощности. `NaN` и `+inf` инвалидируют всю тройку (`bands = None`).
- Базовые метрики: нефинитные значения отбрасываются; отрицательные и выходящие
  за диапазон — ограничиваются (`flatness` в `0.0..=1.0`, частоты неотрицательны).
- При выключении свойство `af` очищается, кадр сбрасывается. Новый `play`, `stop` и
  ошибки также очищают кадр; перед новым `play` полосы снимаются до `FileLoaded`.
- Если `@level` недоступен, mpv возвращает ошибку, команда отдаёт
  `AudioError::Spectrum`; воспроизведение не прерывается. Если отклонён только
  `@spectrum`, воркер остаётся на `@level`, пишет `warn!` и отдаёт `bands = None`.
- Полосы спектра по бинам не поддерживаются; подробности и альтернативы — ADR-0010,
  ADR-0027.

## 8. Ошибки

Все ошибки мапятся в `AudioError` (thiserror).  
`Init` означает, что модуль неработоспособен; Application Core считает это фатальной
ошибкой запуска. Остальные варианты возвращаются из конкретной команды.

## 9. Тестирование

- Unit-тесты: валидация URL (только http/https с хостом), диапазон громкости, backoff,
  исчерпание попыток, сброс трекера, включённость спектра по умолчанию, разбор и
  ограничение FFT-метрик, карта раскладок каналов, форма stereo/mono-графов,
  объединение L/R по мощности и санитизация полос.
- Живой тест `plays_and_stops_live_stream` (`#[ignore]`, требует сеть и аудиоустройство):
  `ORBITFM_TEST_STREAM_URL=... cargo test --all-targets plays_and_stops_live_stream -- --ignored`
  проверяет play → pause → resume → stop.
- Живой тест `produces_fft_metrics_for_live_stream` (`#[ignore]`) включает визуализатор,
  ждёт до 20 с первый `SpectrumFrame` и полосы, проверяет конечность значений,
  выключает фильтр и убеждается, что кадр очищен. Тест выставляет громкость 0.
  Проверено локально на stereo- и mono-потоках.

## 10. Текущие ограничения

- Полосы — три огибающие Bass/Mid/High, а не спектр по бинам; при недоступности
  `@spectrum` остаётся `@level` и `bands = None`.
- Наличие `aspectralstats`, `acrossover` и `amerge` зависит от сборки libavfilter
  (проверено на FFmpeg n9.0.2 / mpv 0.41).
- Метаданные ICY (название трека) не читаются — это задача будущих этапов.
- Поддержка Windows/macOS не проверялась.

## 11. Связанные ADR

- ADR-0003: Выбор аудио-движка
- ADR-0010: Экспериментальный FFT-визуализатор
- ADR-0022: Визуализатор в стиле cava
- ADR-0027: Трёхполосный анализ эфира без PCM-tap
- ADR-0028: Аудио-реактивная деформация глобуса
