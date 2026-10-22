---
name: orbitfm-audio
description: Аудио-подсистема OrbitFM на libmpv. Использовать при работе с воспроизведением, реконнектом, громкостью, событиями аудио и FFI-обёрткой libmpv.
---

# OrbitFM Audio

Ты работаешь с аудио-движком OrbitFM (libmpv).

Опирайся на ADR-0003, `docs/modules/01-audio-engine.md`, `docs/interfaces/01-audio.md`.

## Обязательные правила libmpv

- `LC_NUMERIC` должен быть `"C"`.
- Порядок: `mpv_create` → настройка → `mpv_initialize`.
- Для воспроизведения радио использовать `url_resolved` из Radio Browser.
- libmpv в общем случае не thread-safe — все вызовы через один поток или очередь команд + `spawn_blocking`.

## Контракт

Реализуй трейт `AudioEngine` из `docs/interfaces/01-audio.md`:

- `play`, `stop`, `set_volume`
- `state`, `volume`
- `subscribe` → `AudioEvent`

## Реконнект

- При обрыве потока — автоматический реконнект с backoff.
- Состояние `Reconnecting` должно быть видно подписчикам.
- Не блокировать UI-поток во время реконнекта.

## FFI

- Минимальные `unsafe`-блоки с комментарием «почему безопасно».
- RAII-обёртка с `Drop` для `mpv_handle`.
- Коды возврата C API мапить в `AudioError`.

## PCM / FFT (экспериментально)

- Только через audio-filter / IPC mpv, изолированно.
- Не блокировать основной цикл GUI.

## Чего нельзя

- Вызывать libmpv из произвольных потоков без синхронизации
- Использовать сырой `url` вместо `url_resolved` без причины
- Держать аудиологику в UI или radio-модуле
- Игнорировать ошибки инициализации (лучше упасть явно)
