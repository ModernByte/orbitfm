# Модуль: Media Controls

**Путь в коде:** `src/media/`  
**Ответственность:** Интеграция с системными медиа-клавишами и виджетами (MPRIS v2 на Linux; SMTC/Now Playing — позже).  
**Зависимости:** playwire 2.0 (Linux-бэкенд — MPRIS через zbus), tokio

---

## 1. Назначение

Позволить пользователю управлять воспроизведением через:

- Медиа-клавиши
- Системные UI (GNOME, KDE, Noctalia, `playerctl` и т.д.)
- ОС-специфичные контролы (в перспективе)

Модуль не содержит бизнес-логики: он публикует метаданные в систему и передаёт команды
Application Core.

## 2. Публичный интерфейс

Полное описание — `docs/interfaces/06-media.md`. Кратко:

```rust
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

pub struct MediaControls { /* ... */ }

impl MediaControls {
    pub fn new(wake: WakeFn) -> Result<Self, MediaError>;
    pub fn set_playback(&mut self, info: &PlaybackInfo) -> Result<(), MediaError>;
    pub fn events(&mut self) -> &mut mpsc::UnboundedReceiver<MediaCommand>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: Option<String>,
    pub art_url: Option<String>,
    pub url: String,
    pub playing: bool,
    pub volume: f64,
}

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
```

## 3. Платформенные особенности

- **Linux:** MPRIS v2 через playwire (zbus). Имя — `org.mpris.MediaPlayer2.OrbitFM`.
- **Windows:** System Media Transport Controls — Этап 6.
- **macOS:** MPNowPlayingInfoCenter + remote commands — Этап 6.

## 4. Конфигурация MPRIS

| Параметр | Значение |
|----------|----------|
| Identity | `OrbitFM` |
| DesktopEntry | `org.modernbyte.OrbitFM` |
| Track ID prefix | `/org/modernbyte/OrbitFM/track` |
| Volume | из `PlaybackInfo`, `0.0..=1.0` |
| Capabilities | `CanPlay`, `CanPause`, `CanControl`; `CanGoNext`/`CanGoPrevious`/`CanSeek` выключены |

## 5. Потоки и пробуждение GUI

playwire вызывает колбэк в рабочем потоке D-Bus. Колбэк:

1. преобразует событие в `MediaCommand` (неизвестные и seek игнорируются);
2. кладёт команду в `mpsc::UnboundedSender`;
3. вызывает `WakeFn`, который делает `egui::Context::request_repaint()`.

Без шага 3 команда осталась бы в очереди до следующего пользовательского ввода или
анимации. Application Core разбирает очередь в `logic()` и выполняет аудио-команду.

`set_playback` дедуплицирует состояние: если `PlaybackInfo` не изменился, D-Bus не
трогается. Это важно, потому что `sync_media()` вызывается каждый кадр.

## 6. Соответствие состояний

| `AudioEngine` | `playing` в MPRIS | `PlaybackStatus` |
|---------------|-------------------|------------------|
| `Stopped`, `Error` | `false`, трек отсутствует | `Stopped` |
| `Paused` | `false` | `Paused` |
| `Playing`, `Buffering`, `Reconnecting` | `true` | `Playing` |

Метаданные берутся из текущей станции: `name` → title, `tags` → artist, `favicon` →
artUrl, `url_resolved` → `xesam:url`. Если станция неизвестна, используется URL потока.

## 7. Ограничения

- `Next` / `Previous` принимаются, но не действуют: нет списка станций/истории.
- Перемотка (seek) не поддерживается для живого радио.
- Windows/macOS-бэкенды не проверялись.

## 8. Тестирование

- Unit-тесты маппинга событий playwire, включая клампинг громкости и отказ `NaN`.
- Живая проверка на Linux: `playerctl -p OrbitFM status`, `volume`, `play-pause`,
  `stop`; IPC `status` должен отражать изменения в течение одного кадра.

## 9. Связанные ADR

- ADR-0006: Медиа-контроли
