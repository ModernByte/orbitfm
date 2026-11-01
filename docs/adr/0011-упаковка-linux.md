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
     `objdump`; MSI собирает WiX 5 (последняя версия без OSMF EULA, которую
     требуют v6+) для per-user установки в `%LOCALAPPDATA%\Programs\OrbitFM`
     с ярлыком в меню «Пуск»; CI выполняет тихую установку и удаление MSI.
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
  добавляются runtime DLL MSVC. WiX 5 (v5.0.2) собирает per-user MSI
  (`%LOCALAPPDATA%\Programs\OrbitFM`, ярлык в меню «Пуск»); CI проверяет тихую
  установку и удаление.
- Джоба `dmg` (`macos-latest`): `build-dmg.sh` собирает `.app`, генерирует
  `.icns` из общего SVG, бандлит зависимости через `dylibbundler`, подписывает
  ad-hoc и создаёт UDZO DMG со ссылкой на `/Applications`; CI монтирует образ,
  проверяет подпись и отсутствие ссылок на Homebrew.
- Оба артефакта публикуются через `actions/upload-artifact` и не выкладываются
  во внешние каналы. MSI не подписан Authenticode, DMG не подписан Developer ID
  и не нотаризован — это ограничение до появления сертификатов у владельца.

## Уточнение (2026-10-29): LGPL-2.1, чекеры вендинга и контур поставки

- **LGPL-2.1 у обёрток libmpv.** `libmpv2` и `libmpv2-sys` — единственные
  copyleft-пакеты графа (замер по 558 пакетам `Cargo.lock`). Обязательства
  покрываются динамической линковкой, и ни один артефакт не исключает замену
  библиотеки: замеры локальной сборки показывают `NEEDED libmpv.so.2` в
  бинарнике и `AppDir/usr/lib/libmpv.so.2` отдельным файлом (174 `.so` всего),
  MSI кладёт `libmpv-2.dll` в каталог приложения, DMG несёт `.dylib` в
  `Contents/Frameworks` (бандлинг через `dylibbundler`). Статическая линковка
  libmpv в этих форматах запрещена как класс решений: она превратила бы
  LGPL-артефакт в неисправляемый пользователем бинарник. Релизный workflow
  проверяет это отдельным шагом (`Verify libmpv stays replaceable (LGPL)`),
  потому что «запрещено как класс» без гейта — это пожелание, а не ограничение.
  `deny.toml` держит `LGPL-2.1` в `allow` ровно по этому обоснованию — сам по
  себе copyleft в список не попадает.
- **Чекер вендинга.** `packaging/linux/check-flatpak-sources.py` сверяет
  `flatpak-cargo-sources.json` с `Cargo.lock` в две стороны (равенство множеств
  записей, совпадение `sha256` archive-записи с `checksum` из лока, наличие
  inline `.cargo-checksum.json` с тем же дайджестом и соответствие `url`
  паттерну `static.crates.io`), и печатает число записей, чтобы рост вендинга
  был ревью-абельным. Проверка идёт по `sha256`: замерено, что `checksum` в
  `Cargo.lock`, `sha256` `.crate`-тарбола и поле `sha256` в манифесте совпадают,
  поэтому недостающие записи выводятся офлайн и детерминированно. Сужать чекер
  до Linux-платформы намеренно не стали: JSON генерируется платформенно-
  независимо и вендорит также macOS/Windows-крейты, а сужение легализовало бы
  непроверенные записи.
- **Контур поставки.** `packaging/cargo-deny-check.sh` — единая точка: сначала
  требует, чтобы каждая прощённая запись `[advisories] ignore` имела
  `# revisit: ГГГГ-ММ-ДД` и чтобы ни одна дата пересмотра не истекла, затем
  ставит `cargo-deny` строго пиннованной версии с проверкой SHA256 и гоняет
  `cargo deny check` по графу целиком (без `-t/--target`: платформенные
  зависимости macOS и Windows тоже входят в артефакты). Job `supply-chain` в CI
  и тот же шаг гейтом в релизном workflow — потому что пуш тега CI не
  триггерит, а артефакты не должны уезжать с непроверенным графом.
- **Что здесь не решено.** Подписи артефактов (Authenticode, Developer ID,
  notarization) и detached-сигнатуры к `SHA256SUMS` — по-прежнему зона
  ответственности владельца: сертификат не заведён, поэтому релизы подписываются
  только ad-hoc, а `SHA256SUMS` содержит дайджесты, а не сигнатуры.

## Уточнение (2026-11-01): Flathub, интеграция AppImage и план подписей

### Flathub: что готово и что блокирует публикацию

Локальный манифест `packaging/linux/org.modernbyte.OrbitFM.yml` собирается и
ставится через `build-flatpak.sh`; runtime 25.08, libmpv/libass/libplacebo из
исходников и vendored Cargo не меняются. Для community-публикации в Flathub
нужны ещё четыре вещи:

1. **Удалённый источник вместо `type: dir`.** Flathub собирает приложение не из
   рабочей копии, поэтому на подачу идёт вариант манифеста, где модуль `orbitfm`
   объявлен через git с тегом и полным коммитом релиза:
   ```yaml
   sources:
     - type: git
       url: https://github.com/ModernByte/orbitfm.git
       tag: v<версия>
       commit: <полный SHA коммита тега>
     - flatpak-cargo-sources.json
   ```
   Остальное — модули libplacebo/libass/libmpv, `finish-args` и inline
   `.cargo/config.toml` с `vendored-sources` — переносится без изменений.
   Локальный манифест с `type: dir` остаётся источником истины для разработки:
   второй манифест-дубликат не заводится, чтобы пины версий и вендинга не
   расходились.
2. **Скриншоты в AppStream.** В `org.modernbyte.OrbitFM.metainfo.xml` добавлены
   два снимка из `docs/wiki/images` с абсолютными URL на ветку `main`;
   `appstreamcli validate --no-net` проходит без новых замечаний.
3. **Стабильный тег.** Текущие релизы — `-rc.N`, а основной канал Flathub
   рассчитан на стабильную версию. Подача начинается после решения владельца:
   выпустить стабильный тег либо подавать pre-release отдельным beta-каналом.
4. **Аккаунт и репозиторий Flathub.** Подача — PR в `flathub/<app-id>` от
   аккаунта владельца; токенов и ключей Flathub в репозитории проекта нет.
   Манифест подачи ограничивает архитектуры x86_64 (`flathub.json`,
   `"only-arches": ["x86_64"]`): aarch64 не собирается и не проверяется.

Локальная установка Flatpak по-прежнему не даёт IPC-сокет песочницы наружу,
поэтому виджет Noctalia и команды `orbitfm …` работают только с AppImage или
дистрибутивной сборкой (см. README).

### Интеграция AppImage без root

`packaging/linux/install-appimage.sh` публикуется рядом с AppImage как
`install-appimage.sh` и ставит скачанный образ в меню приложений:

- копирует AppImage в `${XDG_BIN_HOME:-$HOME/.local/bin}/orbitfm` и делает
  копию исполняемой, поэтому пользователю не нужен `chmod +x`;
- распаковывает образ через `--appimage-extract` во временный каталог и
  устанавливает desktop entry, иконки hicolor и metainfo в `XDG_DATA_HOME`;
- правит только строки `Exec=`/`TryExec=` по точному префиксу ключа (без
  регулярных выражений), сохраняя аргументы (`mode focus`) и остальные строки;
  путь с пробелами экранируется по Desktop Entry Specification;
- пишет список установленных файлов в
  `${XDG_STATE_HOME:-$HOME/.local/state}/orbitfm/appimage-install.list`;
  `--uninstall` удаляет только пути внутри сохранённых `BIN_HOME`/`DATA_HOME`;
- обновляет `update-desktop-database` и `gtk-update-icon-cache`, если они есть,
  и предупреждает, если каталог установки отсутствует в `PATH`.

Ограничения: установленный файл остаётся AppImage, поэтому для запуска нужен
FUSE либо `APPIMAGE_EXTRACT_AND_RUN=1`; обновление — повторный запуск скрипта с
новым образом; AppImageLauncher не требуется, но и не конфликтует с ним.

### Подписи: текущее состояние и поэтапный план

Сейчас:

- MSI не подписан Authenticode и не имеет метки времени;
- DMG подписан только ad-hoc (`codesign --sign -`), без Developer ID,
  hardened runtime и notarization;
- Linux-артефакты не имеют detached-подписи: `SHA256SUMS` лежит на той же
  странице релиза и защищает от повреждения, но не от подмены.

План (не реализован: нужны сертификаты и секреты владельца; ключи не попадают в
репозиторий):

1. **Linux — detached-подпись.** Minisign или OpenPGP-подпись над `SHA256SUMS`
   (и, при желании, над каждым артефактом), публичный ключ в README. Подпись
   ставится после сборки всех артефактов и после генерации контрольных сумм,
   иначе подписывается устаревший дайджест. Проверка — одной командой,
   сертификат не нужен. Это первый шаг: он не зависит от платформ и закрывает
   целостность канала.
2. **Windows — Authenticode.** OV- или EV-сертификат, ключ в аппаратном токене
   или облачном HSM (Azure Trusted Signing, DigiCert KeyLocker и аналоги).
   Подписываются исполняемые файлы до сборки MSI и сам MSI, ставится RFC 3161
   timestamp; CI проверяет `signtool verify /pa /all`. EV даёт немедленную
   репутацию SmartScreen, OV набирает её постепенно; до этого SmartScreen
   предупреждает, как описано в README.
3. **macOS — Developer ID и notarization.** Сертификат Developer ID Application
   (платная программа Apple Developer), hardened runtime, подпись
   `codesign --options runtime`, отправка `notarytool submit --wait`, затем
   `stapler staple` и проверка `spctl --assess --type execute`. Секреты —
   Apple ID с app-specific password или App Store Connect API key в CI.
4. **Порядок.** Сначала Linux-подпись, затем Windows и macOS, потому что
   последние два требуют покупки сертификатов и настройки CI-секретов. До
   появления ключей релизы остаются не подписанными коммерческими
   сертификатами, и документация обязана это говорить прямо; самоподписанные
   сертификаты не используются — Gatekeeper и SmartScreen им не доверяют.

После включения подписей `SHA256SUMS` генерируется заново после каждого шага
подписи; порядок «сборка → подпись → дайджесты → detached-подпись дайджестов»
фиксируется в release-workflow.

## Ссылки

- ADR-0002: Выбор GUI-стека
- ADR-0003: Выбор аудио-движка
- ADR-0005: Single-instance и IPC
- ADR-0006: Медиа-контроли
- ADR-0025 (ред. 2): безопасная обработка favicon — политика, из-за которой
  артворк стал единственным сетевым путём с повышенным контролем
- AppImage documentation, linuxdeploy, appimagetool
- Flathub App Submission, Flatpak manifest guidelines
- Desktop Entry Specification, AppStream specification
- minisign, Microsoft Authenticode и RFC 3161, Apple Developer ID и notarization
