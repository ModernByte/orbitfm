# Установка на macOS: Intel

← [Установка](01-Установка) · [Домой](Home)

Эта страница — для Mac на процессоре Intel. Нужен файл
`OrbitFM-0.21.0-rc.3-macos-x86_64.dmg`.

Apple Silicon эту сборку тоже запустит через Rosetta, но нативная arm64
быстрее и не требует Rosetta — для таких Mac есть
[отдельная инструкция](Установка-macOS-Apple-Silicon). Обратное не работает:
arm64-приложение на Intel Mac не запускается вообще, и Rosetta тут не помогает.

Здесь описана установка; чем обеспечены обещания про macOS (пол 13.0, обе арки,
единственное требование к GPU) и что из этого ещё не подтверждено живым прогоном —
в [ADR-0052](https://github.com/ModernByte/orbitfm/blob/main/docs/adr/0052-матрица-поддержки-macos.md). Аппаратная нижняя граница, от
которой мы отталкиваемся, — `MacBookAir8,2` (MacBook Air Retina 13″ 2019, Intel Core
i5-8210Y, Intel UHD Graphics 617, 8 ГБ); всё, что мощнее него, тоже подходит.

## 1. Проверьте Mac

```bash
uname -m
```

Ожидается `x86_64`. Если команда печатает `arm64`, это Apple Silicon — откройте
[инструкцию для Apple Silicon](Установка-macOS-Apple-Silicon).

В «Об этом Mac» архитектуре соответствует строка **«Процессор»** (Intel Core
i5/i7/i9, Xeon и т. п.). Версия системы — macOS 13.0 или новее; на более старой
сборка не запустится, и это не «неофициально», а проверяется при сборке:
`LSMinimumSystemVersion` и `check-min-os.py` не выпустят бандл, где хоть один
Mach-O требует другую macOS.

Про арку полезно знать одну ловушку: `uname -m` отвечает архитектуру текущего
процесса, а не машины — терминал, запущенный через Rosetta на Apple Silicon,
напечатает `x86_64`. Если результат вызывает сомнение, сверяйтесь с моделью в
«Об этом Mac»: identifier вида `MacBookAir8,2` — это Intel.

## 2. Скачайте и проверьте файл

Со страницы релизов <https://github.com/ModernByte/orbitfm/releases> скачайте:

- `OrbitFM-0.21.0-rc.3-macos-x86_64.dmg`;
- `SHA256SUMS` — если хотите сверить контрольную сумму.

В macOS нет `sha256sum` из GNU coreutils, зато есть `shasum`. Проверка только
своего файла, без остальных ассетов релиза:

```bash
cd ~/Downloads
grep 'macos-x86_64\.dmg$' SHA256SUMS > orbitfm-x86_64.sha256
shasum -a 256 -c orbitfm-x86_64.sha256
rm orbitfm-x86_64.sha256
```

Ожидаемый вывод — `OrbitFM-0.21.0-rc.3-macos-x86_64.dmg: OK`. Сумма подтверждает,
что файл скачался целиком и совпадает с опубликованным; подписью она не является.

## 3. Установите приложение

1. Откройте `.dmg` двойным кликом.
2. Перетащите **OrbitFM** в папку **Applications**.
3. Извлеките образ: правый клик по нему на рабочем столе → **Извлечь** (Eject).

Приложение ставится в `/Applications/OrbitFM.app`; права администратора не нужны.

## 4. Первый запуск и Gatekeeper

Подпись у сборки только ad-hoc (`codesign --sign -`), нотаризации Apple нет.
Поэтому Gatekeeper предупредит при первом запуске — это ожидаемо, а не признак
вируса.

1. Откройте **Applications**, сделайте правый клик по **OrbitFM** → **Открыть**.
2. В диалоге ещё раз нажмите **Открыть**.

Если кнопки **Открыть** нет (так бывает на macOS 15 и новее):

1. Попробуйте запустить приложение обычным двойным кликом.
2. Откройте **Системные настройки** → **Конфиденциальность и безопасность**.
3. Прокрутите вниз до сообщения об OrbitFM и нажмите **Всё равно открыть**
   (Open Anyway), подтвердите паролем или Touch ID.

Запасной путь из терминала — снять карантин с приложения:

```bash
xattr -d com.apple.quarantine /Applications/OrbitFM.app
open /Applications/OrbitFM.app
```

Если macOS пишет, что приложение **«повреждено»**, это обычно тот же карантин у
ad-hoc подписи. Повторите `xattr -dr com.apple.quarantine /Applications/OrbitFM.app`,
затем запустите снова. Если сообщение осталось — перекачайте DMG и сверьте сумму.

## 5. Команды `orbitfm` в терминале

Приложению в `/Applications` соответствует бинарник
`/Applications/OrbitFM.app/Contents/MacOS/orbitfm`. Чтобы работали `orbitfm status`,
`orbitfm panel`, виджет Noctalia и остальные команды, положите его в `PATH`:

```bash
mkdir -p ~/.local/bin
ln -sf /Applications/OrbitFM.app/Contents/MacOS/orbitfm ~/.local/bin/orbitfm
```

Проверьте, что `~/.local/bin` есть в `PATH`:

```bash
echo $PATH | tr ':' '\n' | grep "$HOME/.local/bin"
```

Если строки нет, добавьте каталог в `~/.zshrc` и перезапустите терминал:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

Проверка установки:

```bash
orbitfm --version
file /Applications/OrbitFM.app/Contents/MacOS/orbitfm
```

Ожидается версия `0.21.0-rc.3` и `Mach-O 64-bit executable x86_64`.

### Если картинка не появилась — снять лог рендера

Первый кадр пишется в stderr одной строкой `orbitfm render: …` с тем GPU, который
выбрал wgpu, его драйвером и лимитами. Строка — диагностика, а не интерфейс: её формат
может меняться между релизами. Чтобы собрать доказательство для приёмки или для
[issue](https://github.com/ModernByte/orbitfm/issues):

```bash
orbitfm quit
orbitfm 2>&1 | tee orbitfm-render.log
```

Закройте окно (или выполните `orbitfm quit` в другом терминале) — и пришлите
`orbitfm-render.log` целиком. Если строки `orbitfm render:` нет вовсе, приложение не
дошло до инициализации рендера, и это уже другой дефект: тогда полезен вывод
`RUST_LOG=debug orbitfm 2>&1 | head -50`.

## 6. Обновление и удаление

**Обновление.** Скачайте новый DMG, завершите текущий процесс командой `orbitfm quit`,
перетащите новое приложение в **Applications** с заменой. Настройки, избранное и
история лежат отдельно и сохраняются.

**Удаление.**

```bash
orbitfm quit
rm -f ~/.local/bin/orbitfm
```

Затем перетащите `OrbitFM.app` из **Applications** в **Корзину**. Если хотите удалить и
пользовательские данные:

```bash
rm -rf ~/.config/orbitfm ~/.local/share/orbitfm
```

## 7. Сборка из исходников

Нужны Xcode Command Line Tools и Rust 1.95+. Homebrew и `brew install mpv`
не нужны: скрипт скачивает закреплённые бутылки `libmpv`/`dylibbundler` из
ghcr.io, разворачивает их в префикс и пишет `LIBRARY_PATH`.

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
git clone https://github.com/ModernByte/orbitfm
cd orbitfm
python3 packaging/macos/fetch-libmpv.py
export LIBRARY_PATH="/tmp/orbitfm/opt/mpv/lib"
cargo build --release --locked
./target/release/orbitfm
```

DMG из этого же дерева — арка объявляется флагом, а не выводится из машины сборки:

```bash
bash packaging/macos/build-dmg.sh --arch x86_64
```

Без `--arch` скрипт остановится с сообщением об использовании (так же, как и с
неизвестным значением) — это замена прежнему поведению, когда арка молча бралась из
`uname -m` и сборка на чужой архитектуре оставляла `.app` не того среза.

Готовый файл появится в `dist/OrbitFM-0.21.0-rc.3-macos-x86_64.dmg`. Внутри — `libmpv`
из бутылок Homebrew под macOS 13, бандлится в `.app` (путь к `dylibbundler` задаётся
через `PATH`, он создаётся скриптом в префиксе). Перед тем как положить файл в `dist/`,
бандл проходят гейты, встроенные в скрипт: `check-app-arch.sh` (каждый Mach-O обязан
иметь ровно объявленную арку — ловит и случайную подмену, и fat-бинарник),
`audit-app-dylibs.sh` (пути зависимостей) и `smoke-launch.sh` (реальный запуск
`orbitfm --version`). Ещё один, `check-min-os.py` — ни один Mach-O не требует macOS
выше обещанного, предел берётся из `Info.plist`, а не из командной строки, — исполняется
в CI и в релизном workflow, где есть с чем сравнивать весь бандл.

## Если что-то не работает

| Симптом | Причина | Что сделать |
|---|---|---|
| «Нельзя открыть программу… не поддерживается этим компьютером Mac» | Скачан DMG не той архитектуры (arm64 на Intel) или macOS старше 13.0 | `uname -m` → `x86_64`; скачайте `macos-x86_64.dmg`; обновите macOS до 13+ |
| «Для программы требуется macOS 13.0 или новее» | Старая macOS | Обновите систему; сборка под более старую macOS не публикуется |
| «Разработчик не может быть проверен» | Нет нотаризации Apple | Правый клик → «Открыть» либо «Всё равно открыть» в настройках |
| «Приложение повреждено» | Карантин у ad-hoc подписи | `xattr -dr com.apple.quarantine /Applications/OrbitFM.app` |
| `orbitfm: command not found` | Бинарника нет в `PATH` | Симлинк в `~/.local/bin/orbitfm`, проверить `echo $PATH` |
| Нет иконки в строке меню | Трей macOS не создался | `RUST_LOG=debug` и [Трей и MPRIS](07-Трей-и-MPRIS); приложение работает и без иконки |
| Окно открылось, но глобуса нет / он чёрный | Рендер не инициализировался либо GPU не потянул лимиты | снять `orbitfm-render.log` (раздел 5) и прислать его; рендер на Intel UHD 617 живой приёмкой ещё не подтверждён |
| Приложение падает сразу после открытия, окна нет | dyld не собрал граф зависимостей | `orbitfm --version` из терминала; если abort или `Symbol not found` — приложить вывод целиком, см. [Неполадки](09-Неполадки) |
| Нет звука | Сеть, кодек или адрес станции | [Неполадки](09-Неполадки) |

---

Далее: [Первый запуск](02-Первый-запуск) · [Установка на Apple Silicon](Установка-macOS-Apple-Silicon) ·
[Трей и MPRIS](07-Трей-и-MPRIS)
