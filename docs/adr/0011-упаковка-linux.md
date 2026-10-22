# ADR-0011: Поставка Linux и упаковка

**Статус:** Принято  
**Дата:** 2026-10-22  
**Участники решения:** ModernByte Architecture, Юрий Анатольевич  

## Контекст

Этап 6 требует упаковки AppImage / Flatpak / MSI / DMG. Фактическое окружение
разработки — Linux x86_64 (CachyOS, Wayland, mpv 0.41 / FFmpeg 12.1). Для Windows и
macOS отсутствуют нативные раннеры, SDK и сборки libmpv; кросс-проверка `cargo check`
упирается в C-зависимости (`aws-lc-sys` требует mingw-компилятор). Кроме того, в коде
пока нет Windows named pipe и привязки SMTC к HWND, а macOS Now Playing требует
запущенного `NSApplication` в момент регистрации.

Flatpak требует воспроизводимой сборки libmpv и vendored cargo-исходников; в окружении
нет `flatpak-builder`. MSI и DMG нельзя собрать и проверить без Windows/macOS.

При этом метаданные рабочего стола (desktop entry, иконка, AppStream) нужны для любого
формата поставки и не зависят от платформенных бэкендов.

## Решение

1. Единый источник метаданных рабочего стола — `packaging/linux/`:
   `org.modernbyte.OrbitFM.desktop`, `org.modernbyte.OrbitFM.metainfo.xml` и
   масштабируемая иконка `org.modernbyte.OrbitFM.svg`.
2. Основной проверяемый артефакт Linux — **AppImage**, который собирает
   `packaging/linux/build-appimage.sh`: release-сборка, `linuxdeploy` (бандлинг
   динамических библиотек, включая libmpv и FFmpeg) и `appimagetool`.
   Результат — `target/appimage/OrbitFM-<version>-x86_64.AppImage`.
3. Инструменты AppImage не входят в репозиторий: скрипт использует `LINUXDEPLOY` и
   `APPIMAGETOOL` из окружения либо скачивает continuous-сборки в
   `target/appimage-tools`. Для сред без FUSE применяется
   `APPIMAGE_EXTRACT_AND_RUN=1`.
4. CI проверяет метаданные (`desktop-file-validate`, `appstreamcli validate`,
   `bash -n`, XML/plist); сборка AppImage остаётся локальной/ручной до появления
   release-workflow.
5. **Отложено до появления платформенных бэкендов (MSI/DMG разблокированы
   2026-10-23, см. уточнение ниже):**
   - Windows (SMTC) — привязка playwire к HWND;
   - macOS (Now Playing) — рантайм-проверка Now Playing.

   Критерий возврата: зелёная сборка и ручная проверка медиа-интеграции на
   соответствующей ОС. До этого пункты ROADMAP остаются незакрытыми с пометкой
   «заблокировано».
6. **MSI и DMG в CI (2026-10-23).** Нативные установщики собираются отдельными
   джобами и публикуются артефактами `actions/upload-artifact`; во внешние
   каналы не выкладываются:
   - Windows: MSYS2 UCRT64 `mingw-w64-ucrt-x86_64-mpv` (libmpv 0.41.0); импорт
     `mpv.lib` генерируется из `libmpv-2.dll` через `dumpbin` и `lib.exe`;
     MSVC release-сборка; транзитивные DLL собираются по таблицам импорта через
     `objdump`; MSI собирает WiX 7 (элемент `Files` появился только в v7) для
     per-user установки в `%LOCALAPPDATA%\Programs\OrbitFM` с ярлыком в меню
     «Пуск»; CI выполняет тихую установку и удаление MSI.
   - macOS: `.app` с `Info.plist`, иконкой `.icns` (из общего SVG через
     `rsvg-convert`/`iconutil`), зависимости бандлятся `dylibbundler`, подпись
     ad-hoc; DMG (UDZO) содержит приложение и ссылку на `/Applications`;
     CI монтирует DMG и проверяет подпись и отсутствие ссылок на Homebrew.
   - Артефакты не подписаны сертификатами и не нотаризованы: MSI не имеет
     Authenticode, DMG — Developer ID и notarization. Это осознанное ограничение
     до появления сертификатов у владельца.

## Альтернативы

| Вариант | Причина отклонения |
|---------|--------------------|
| Только tar.gz | Не соответствует ROADMAP и не даёт интеграции с рабочим столом |
| Flatpak первым | Воспроизводимее, но в текущем окружении не собирается и требует модуля libmpv |
| Статическая сборка musl | libmpv, wgpu и Wayland/EGL делают её непрактичной |
| Пакеты дистрибутивов (deb/rpm/AUR) | Отдельная инфраструктура и поддержка; возможна позже |

## Последствия

**Положительные:**

- Есть проверяемый переносимый артефакт для Linux
- Метаданные рабочего стола переиспользуются Flatpak и дистрибутивными пакетами
- Блокеры кроссплатформенности зафиксированы явно, а не замаскированы «зелёным» чекбоксом

**Отрицательные / Риски:**

- AppImage наследует ограничения хостового libmpv/FFmpeg и драйверов; проверен только
  на x86_64 Linux
- Continuous-инструменты не закреплены по хешу; для релизов потребуется pinning
- MSI/DMG собираются без подписи и нотаризации; для релиза потребуются сертификаты
  владельца

## Уточнение (2026-10-22): Flatpak

Flatpak разблокирован и реализован: `packaging/linux/org.modernbyte.OrbitFM.yml`,
`packaging/linux/flatpak-cargo-sources.json` и `packaging/linux/build-flatpak.sh`.

- Runtime — `org.freedesktop.Platform` 25.08, SDK — `org.freedesktop.Sdk` 25.08
  с расширением `org.freedesktop.Sdk.Extension.rust-stable`.
- libmpv 0.41.0, libass 0.17.5 и libplacebo 7.360.1 собираются из исходников.
  FFmpeg 7.1, GnuTLS, PulseAudio и PipeWire берутся из runtime/SDK; отдельная
  сборка FFmpeg не нужна.
- Cargo-исходники зафиксированы генератором `flatpak-cargo-generator` по
  `Cargo.lock`; сборка идёт с `--offline`.
- Sandbox: сеть, Wayland с fallback на X11, PulseAudio, PipeWire, DRI и
  собственное имя MPRIS `org.mpris.MediaPlayer2.OrbitFM`.
- `build-flatpak.sh` собирает через `flatpak-builder` или `org.flatpak.Builder`,
  экспортирует в `target/flatpak-repo` и ставит в пользовательскую установку.
- Проверено локально: сборка, запуск окна, воспроизведение станции, MPRIS
  (`Playing`) и IPC-команды через второй запуск Flatpak. IPC-сокет Flatpak
  находится в приватном runtime-каталоге sandbox, поэтому Noctalia-виджет и
  внешний CLI работают только с нативной установкой.
- Публикация в Flathub не выполняется: артефакт собирается локально, как и
  AppImage; внешняя публикация потребует отдельного решения владельца.

## Уточнение (2026-10-23): нативные CI-джобы

- macOS (`macos-latest`): libmpv из Homebrew, `fmt`, `clippy`, тесты и release-сборка.
- Windows (`windows-latest`): `fmt` и `clippy`; проверяется компиляция. Реализован
  named pipe IPC (`src/ipc/pipe.rs`).
- MSI и DMG на этом шаге ещё не собирались; ниже добавлены отдельные джобы упаковки.

## Уточнение (2026-10-23): MSI и DMG

- Джоба `msi` (`windows-latest`): MSYS2 UCRT64 с `mingw-w64-ucrt-x86_64-mpv`
  (libmpv 0.41.0). `prepare-libmpv.ps1` извлекает экспорты через `dumpbin` и
  собирает `mpv.lib` через `lib.exe`; `RUSTFLAGS=-L native=...` указывает путь
  MSVC-линкеру. `stage-app.sh` обходит DLL-замыкание через `objdump`, после чего
  добавляются runtime DLL MSVC. WiX 7 собирает per-user MSI
  (`%LOCALAPPDATA%\Programs\OrbitFM`, ярлык в меню «Пуск»); CI проверяет тихую
  установку и удаление.
- Джоба `dmg` (`macos-latest`): `build-dmg.sh` собирает `.app`, генерирует
  `.icns` из общего SVG, бандлит зависимости через `dylibbundler`, подписывает
  ad-hoc и создаёт UDZO DMG со ссылкой на `/Applications`; CI монтирует образ,
  проверяет подпись и отсутствие ссылок на Homebrew.
- Оба артефакта публикуются через `actions/upload-artifact` и не выкладываются
  во внешние каналы. MSI не подписан Authenticode, DMG не подписан Developer ID
  и не нотаризован — это ограничение до появления сертификатов у владельца.

## Ссылки

- ADR-0002: Выбор GUI-стека
- ADR-0003: Выбор аудио-движка
- ADR-0005: Single-instance и IPC
- ADR-0006: Медиа-контроли
- AppImage documentation, linuxdeploy, appimagetool
- Desktop Entry Specification, AppStream specification
