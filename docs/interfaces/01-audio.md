# Интерфейс: Audio Engine

Реализация — конкретная структура `AudioEngine` (`src/audio/`), публичный трейт не
используется. Методы, изменяющие состояние, асинхронные; команды выполняются в выделенном
аудио-потоке с таймаутом 3 с. Ошибка команды воспроизведения не меняет состояние молча:
движок публикует соответствующее событие. Ошибка визуализатора возвращается вызывающему
и не влияет на воспроизведение.

```rust
use tokio::sync::broadcast;

/// Состояние воспроизведения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Reconnecting,
    Error(String),
}

impl PlaybackState {
    /// Короткое имя для IPC-статуса.
    pub fn as_str(&self) -> &'static str;
    // stopped | playing | paused | buffering | reconnecting | error
}

/// События аудио-движка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEvent {
    Started { url: String },
    Stopped,
    Buffering,
    Reconnecting { attempt: u32 },
    Error { message: String },
    EndOfFile,
}

/// Уровни трёх полос, дБFS. `None` в `SpectrumFrame` — полосы недоступны.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandLevels {
    /// 20–150 Гц.
    pub bass_db: f32,
    /// 150–2000 Гц.
    pub mid_db: f32,
    /// 2000–20000 Гц.
    pub high_db: f32,
}

/// Агрегированные FFT-метрики текущего аудиокадра.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumFrame {
    /// Спектральный центроид, Гц.
    pub centroid_hz: f32,
    /// Спектральный разброс, Гц.
    pub spread_hz: f32,
    /// Частота rolloff (85 %), Гц.
    pub rolloff_hz: f32,
    /// Спектральная плоскостность `0.0..=1.0`.
    pub flatness: f32,
    /// Спектральный поток (ненормированная величина).
    pub flux: f32,
    /// RMS-уровень, дБFS.
    pub rms_db: f32,
    /// Уровни Bass/Mid/High; `None`, если граф полос недоступен (ADR-0027).
    pub bands: Option<BandLevels>,
}

/// Ошибки аудио-подсистемы.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("не удалось инициализировать libmpv: {0}")]
    Init(String),
    #[error("ошибка воспроизведения: {0}")]
    Playback(String),
    #[error("некорректный URL")]
    InvalidUrl,
    #[error("внутренняя ошибка: {0}")]
    Internal(String),
    #[error("визуализатор недоступен: {0}")]
    Spectrum(String),
}

/// Аудио-движок на базе libmpv.
pub struct AudioEngine { /* ... */ }

impl AudioEngine {
    /// Создаёт libmpv-контекст и запускает выделенный аудио-поток.
    pub fn new() -> Result<Self, AudioError>;

    /// Запускает воспроизведение URL (только http/https с непустым хостом).
    pub async fn play(&mut self, url: &str) -> Result<(), AudioError>;

    /// Останавливает воспроизведение; состояние переходит в `Stopped`.
    pub async fn stop(&mut self) -> Result<(), AudioError>;

    /// Ставит на паузу или возобновляет; требует активного URL.
    pub async fn set_paused(&mut self, paused: bool) -> Result<(), AudioError>;

    /// Громкость `0.0..=1.0`; начальное значение — 0.7.
    pub async fn set_volume(&mut self, volume: f64) -> Result<(), AudioError>;

    /// Текущее состояние.
    pub fn state(&self) -> PlaybackState;

    /// Текущая громкость.
    pub fn volume(&self) -> f64;

    /// Подписка на события.
    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent>;

    /// Включает или выключает аудио-анализ (шесть агрегатов и полосы).
    /// По умолчанию включён; при старте воркер сам ставит `af`-фильтры.
    pub async fn set_spectrum_enabled(&mut self, enabled: bool) -> Result<(), AudioError>;

    /// Включён ли аудио-анализ.
    pub fn spectrum_enabled(&self) -> bool;

    /// Последний кадр метрик и полос; `None`, если данных ещё нет.
    pub fn spectrum(&self) -> Option<SpectrumFrame>;

    /// Включает экономный профиль буфера или возвращает умолчания libmpv.
    /// Команда без ответа: ошибка свойства только логируется (ADR-0043 § 3).
    pub fn set_cache_profile(&self, economy: bool);
}
```

## 1. Семантика команд

| Команда | Успех | Ошибки |
|---------|-------|--------|
| `play(url)` | состояние `Buffering`, событие `Buffering`; после загрузки — `Playing`/`Paused` и `Started` | `InvalidUrl`, `Playback` (mpv не принял `loadfile`), `Internal` (поток остановлен/таймаут) |
| `stop()` | состояние `Stopped`, событие `Stopped` | `Playback`, `Internal` |
| `set_paused(true/false)` | `Paused` / `Playing`; при возобновлении публикуется `Started` | `Playback`, если нет активного URL или состояние `Stopped`/`Reconnecting`/`Error` |
| `set_volume(v)` | значение применяется к mpv и к `volume()` | `Playback`, если `v` не конечное число или вне `0.0..=1.0` |
| `set_spectrum_enabled(true/false)` | цепочка `@level` + `@spectrum` подключается к `af` или снимается; при выключении кадр очищается; состояние воспроизведения не меняется; на libmpv ниже 0.40 включение подтверждается без установки фильтров | `Spectrum`, если mpv отклонил обе цепочки (даже `@level`); отказ только `@spectrum` даёт fallback без ошибки |
| `set_cache_profile(true/false)` | `true` ставит `demuxer-max-bytes=256 МиБ` и `cache-secs=60`; `false` возвращает значения, снятые при старте до первого изменения; повторное значение — no-op; воспроизведение и состояние не меняются | нет: команда без ответа, ошибка `set_property` только логируется |

При старте `spectrum_enabled()` возвращает `true` на libmpv 0.40 и новее, а
воркер сразу применяет фильтры: визуализатор работает без дополнительного
вызова. Значение не сохраняется между запусками и действует до конца сессии.

На libmpv 0.37 и старше воркер видит `mpv-version` ниже порога и оставляет
анализ выключенным: `spectrum_enabled()` возвращает `false`, `@level` и
`@spectrum` не устанавливаются. Это защита от падения в `mpv_get_property`
при чтении `af-metadata` (ADR-0027); воспроизведение не затрагивается.

## 2. Реконнект

- При обрыве активного потока движок переходит в `Reconnecting` и публикует
  `Reconnecting { attempt }`.
- Не более 5 попыток; задержки — 1, 2, 4, 8 и 16 с.
- После исчерпания попыток состояние — `Error(message)`, событие `Error`.
- Успешная загрузка (`FileLoaded`) сбрасывает счётчик попыток.
- `stop()` и новый `play()` также сбрасывают счётчик.

Естественное завершение потока даёт `EndOfFile`, после чего запускается та же политика
реконнекта (для живого радио это эквивалентно обрыву).

## 3. Аудио-анализ

Анализ экспериментальный, включён по умолчанию и работает без перехвата PCM
(ADR-0010, ADR-0027). При включении движок выставляет свойство mpv `af` из двух
независимых размеченных lavfi-фильтров:

```text
@level:lavfi=[
  aspectralstats=measure=centroid+spread+flatness+flux+rolloff:win_size=2048,
  astats=metadata=1:reset=1
],
@spectrum:lavfi=[
  asplit=2[main][analysis];
  [analysis]highpass=f=20,acrossover=split=150 2000:order=8th[lo][mi][hi];
  [lo][mi][hi]amerge=inputs=3[bands];
  [main][bands]amerge=inputs=2,
  astats=metadata=1:reset=1:measure_perchannel=RMS_level:measure_overall=none,
  pan=stereo|c0=c0|c1=c1
]
```

- `@level` сохраняет шесть агрегатов и их ключи
  `af-metadata/level/lavfi.aspectralstats.1.*`, `af-metadata/level/lavfi.astats.Overall.RMS_level`.
- `@spectrum` считает полосы по копии сигнала и возвращает на выход исходную ветку:
  звук остаётся бит-в-бит тем же. Карта каналов: 1–2 — основной сигнал,
  3–4 — Bass (<150 Гц), 5–6 — Mid (150–2000 Гц), 7–8 — High (>2000 Гц).
- Для моно-потока анализ дублирует канал в стерео перед `asplit` и возвращает
  `pan=mono|c0=0.5*c0+0.5*c1`; карта каналов 3–8 не меняется. Для остальных
  раскладок (5.1 и т.п.) остаётся только `@level`, `bands = None`.

Раскладка читается из `audio-params/channel-count` при `FileLoaded` и при включении
анализа. Эмпирически свойство может быть `nil` в момент `FileLoaded`, поэтому воркер
повторяет чтение до 8 раз с интервалом 250 мс; если раскладка так и не определилась,
остаётся `@level`. Перед новым `play()` и при `stop()` полосы снимаются до загрузки
следующего файла.

Поток `orbitfm-audio` опрашивает метаданные примерно раз в 50 мс, пока состояние —
`Playing` или `Buffering`, и публикует `SpectrumFrame`:

- `centroid_hz`, `spread_hz`, `rolloff_hz` — частотные характеристики, Гц;
- `flatness` — плоскостность спектра, нормирована в `0.0..=1.0`;
- `flux` — ненормированный спектральный поток;
- `rms_db` — уровень RMS в дБFS (`0` — полная шкала);
- `bands` — `Some(BandLevels)` с RMS Bass/Mid/High в дБFS либо `None`.

Уровень полосы объединяет L/R средним по мощности, а не средним в дБ. `-inf`
(полностью подавленная полоса) ограничивается снизу `-90.0` дБ; `NaN` и `+inf`
инвалидируют всю тройку и дают `bands = None`. Базовые шесть метрик сохраняют
строгую проверку: любой нефинитный агрегат отбрасывает кадр целиком.

Вне активного воспроизведения, при `play`, `stop` и после ошибки кадр очищается.
Полосы спектра по бинам не предоставляются: доступны только агрегированные метрики
и три огибающие. Если сборка mpv не содержит `aspectralstats` (или отклоняет
`@level`), включение возвращает `AudioError::Spectrum`, а воспроизведение
продолжается. Если отклонён только `@spectrum` (нет `acrossover`/`amerge`),
движок остаётся на `@level`, пишет предупреждение, возвращает успех и отдаёт
`bands = None` до повторного включения визуализатора.

`set_spectrum_enabled` — не только пользовательский тумблер: Application Core
вызывает его на границах окна (ADR-0018). Перед парковкой анализ снимается, при
показе возвращается, поэтому скрытый эфир платит только за декод и вывод
(≈2 % одного ядра на живом потоке вместо ≈19–23 % с полным графом).

## 4. Потоки и жизненный цикл

- Все вызовы libmpv выполняются только в потоке `orbitfm-audio`.
- `AudioEngine::Drop` отправляет команду `Shutdown` и дожидается завершения потока.
- События доставляются через `broadcast`-канал ёмкостью 64; отставший подписчик получает
  `RecvError::Lagged`, а не блокирует движок.

## 5. Экономный профиль буфера

`set_cache_profile(true)` повышает устойчивость к плохому каналу: `demuxer-max-bytes`
становится 256 МиБ, `cache-secs` — 60 с. `false` возвращает не зашитые константы, а
значения libmpv, снятые при создании воркера до первого изменения; если свойство
прочитать не удалось, оно остаётся как есть, а в лог пишется debug. Переключение
идемпотентно: повтор того же значения не трогает mpv.

Команда не ждёт ответа и не влияет на состояние воспроизведения: сбой `set_property`
логируется как предупреждение, эфир продолжается. Когда включать и выключать профиль
(попытка реконнекта ≥ 2, буферизация дольше 3 с, смена станции), решает
`Session`/`EconomyState` — контракт в `docs/interfaces/10-session.md`, ADR-0043 § 3.
