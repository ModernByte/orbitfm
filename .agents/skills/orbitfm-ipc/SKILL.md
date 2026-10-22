---
name: orbitfm-ipc
description: Single-instance и IPC OrbitFM. Использовать при работе с сокетом, named pipe, протоколом команд, CLI и жизненным циклом процесса.
---

# OrbitFM IPC

Ты работаешь с single-instance и IPC.

Опирайся на ADR-0005, `docs/modules/05-ipc.md`, `docs/interfaces/05-ipc.md`.

## Модель

- Один процесс владеет аудио, IPC-сервером и GUI.
- При запуске: если сокет занят → отправить команду и выйти.
- Закрытие окна = Hide, не quit.

## Транспорт

| Платформа | Механизм | Адрес |
|-----------|----------|-------|
| Linux | Unix Domain Socket | `$XDG_RUNTIME_DIR/orbitfm.sock` |
| Windows | Named Pipe | `\\.\pipe\orbitfm` |
| macOS | Unix Domain Socket | runtime-директория |

## Протокол

Текстовый, одна команда = одна строка (`\n`).

Команды: `toggle`, `show`, `hide`, `play <uuid>`, `stop`, `status`, `volume <0..1>`, `quit`.

Ответы: `ok` / `error <msg>` / многострочный `status`.

## Контракт

Реализуй `IpcServer`:

- `try_bind()` → `AlreadyRunning` если экземпляр есть
- `send_to_existing(cmd)`
- `commands()` → поток `IpcCommand`

## Чего нельзя

- Создавать второй процесс для GUI
- Использовать HTTP localhost без необходимости
- Забывать обработку «зомби»-сокетов
- Класть бизнес-логику внутрь IPC-модуля
