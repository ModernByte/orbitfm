# Интерфейс: Media Controls

Реализация — конкретная структура `MediaControls` на базе playwire (MPRIS v2 через
D-Bus на Linux). Публичный трейт не используется. Колбэк playwire выполняется в рабочем
потоке D-Bus и не должен блокироваться: он только кладёт команду в неограниченную очередь
и вызывает `wake`, чтобы GUI-цикл обработал её на ближайшем кадре.

```rust
/// Пробуждение GUI-цикла после команды от системного медиа-виджета.
pub type WakeFn = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Информация, отображаемая системой.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: Option<String>,
    pub art_url: Option<String>,
    pub url: String,
    pub playing: bool,
    pub volume: f64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

/// Команды, приходящие от системы.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    SetVolume(f64),
}

/// Ошибки медиа-контролов.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("не удалось инициализировать медиа-контролы: {0}")]
    Init(String),
    #[error("не удалось обновить метаданные: {0}")]
    Publish(String),
    #[error("платформа не поддерживается")]
    UnsupportedPlatform,
}

/// Обёртка над системными медиа-контролами.
pub struct MediaControls { /* ... */ }

impl MediaControls {
    /// Регистрирует MPRIS-имя и начинает принимать команды системы.
    pub fn new(wake: WakeFn) -> Result<Self, MediaError>;

    /// Обновляет состояние, отображаемое системой.
    /// Повторный вызов с идентичными данными не обращается к D-Bus.
    pub fn set_playback(&mut self, info: &PlaybackInfo) -> Result<(), MediaError>;

    /// Очередь команд от системы.
    pub fn events(&mut self) -> &mut tokio::sync::mpsc::UnboundedReceiver<MediaCommand>;
}
```

## Семантика

| Поле / команда | Поведение |
|----------------|-----------|
| `title`, `artist`, `art_url` | метаданные `Track`; пустой `title` означает отсутствие трека. `art_url` заполняется только адресом, прошедшим гейт публикации (ярус 1 синхронно + свежий вердикт яруса 2, ADR-0025 ред. 3); нет вердикта — пустая строка на этот кадр, резолв идёт фоном. `media` сам проверку не делает и сетевой осведомлённости не имеет — фильтр обязан применить вызывающий слой |
| `url` | идентификатор трека и `xesam:url`; `url_resolved` текущей станции под тем же гейтом, при его пустоте — URL потока из сессии (тоже после гейта), иначе пустая строка |
| `playing` | `true` → `PlaybackStatus = Playing`, `false` → `Paused` при наличии трека и `Stopped` без него |
| `volume` | `0.0..=1.0`, передаётся в MPRIS `Volume` |
| `can_go_next`, `can_go_previous` | рекламируются в MPRIS `CanGoNext`/`CanGoPrevious`; Application Core выставляет их по непустой видимой очереди |
| `Play` | возобновление или повторный запуск текущего URL |
| `Pause` | пауза активного потока |
| `PlayPause` | переключение в зависимости от состояния |
| `Stop` | остановка воспроизведения |
| `SetVolume` | конечное значение приводится к `0.0..=1.0` |
| `Next` / `Previous` | принимаются и исполняются: шаг по текущему видимому списку станций (`play_relative(±1)`, `src/session/playback.rs`) с циклом (wrap); при пустом списке — no-op |

`set_playback` возвращает `Publish`, если playwire не смог обновить состояние. Application
Core логирует ошибку и продолжает работу: отсутствие MPRIS не фатально.

Capabilities: `CanPlay`, `CanPause`, `CanControl`, а также `CanGoNext`/`CanGoPrevious`,
когда видимая очередь непуста. Перемотка (`seek`) выключена по существу:
`can_seek = false`, а `SeekBy` отбрасывается в обработчике — перемотка живого радио
не поддерживается.

`Next`/`Previous` принимаются и действуют по видимому списку, поэтому способности
совпадают с фактическим поведением: при пустой очереди команды — no-op и
`CanGoNext`/`CanGoPrevious` выключены.
