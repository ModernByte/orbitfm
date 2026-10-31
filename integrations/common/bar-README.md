# OrbitFM для баров

`orbitfm-bar` — тонкий мост между OrbitFM и панелью: он читает
`orbitfm status`, печатает подпись в формате бара и отправляет команды по
кликам. Установщик OrbitFM кладёт скрипт и этот файл в каталог выбранной
цели, например `$XDG_DATA_HOME/orbitfm/integrations/waybar/`.

Конфиги оболочек установщик не меняет: нужный сниппет печатается командой
`orbitfm integration install <цель>` и лежит рядом со скриптом.

## Режимы

| Режим | Для чего | Формат вывода |
|---|---|---|
| `text` | апплеты вывода команды (XFCE, Cinnamon, MATE, LXQt) | одна строка текста |
| `polybar` | Polybar `custom/script` | одна строка текста |
| `waybar` | Waybar `custom` | JSON `text`/`tooltip`/`class` |
| `i3blocks` | i3blocks | одна строка текста |
| `swaybar` | `status_command` в swaybar/i3bar | поток i3bar JSON |
| `i3status-rust` | блок `custom` с `json = true` | JSON `text`/`state` |
| `genmon` | xfce4-genmon-plugin | XML `txt`/`tool`/`txtclick` |

Интервал опроса по умолчанию — 2 секунды. Его можно изменить переменной
окружения `ORBITFM_BAR_INTERVAL` (целые секунды).

## Управление

| Действие | Waybar / Polybar / i3blocks / swaybar | XFCE genmon |
|---|---|---|
| ЛКМ | `orbitfm playpause` | `orbitfm playpause` |
| ПКМ | `orbitfm panel` | недоступно в тексте; повесьте ярлык `orbitfm panel` |
| Колесо вверх | `orbitfm volume-up` | — |
| Колесо вниз | `orbitfm volume-down` | — |

Для i3status-rust клики задаются в `[[block.click]]` сниппета.
Cinnamon, MATE и LXQt показывают вывод командного апплета; управление
удобно вынести на отдельные кнопки/ярлыки с командами `orbitfm playpause`
и `orbitfm panel`.

## Безопасность

Скрипт не читает пользовательские файлы и не меняет конфиги. Он вызывает
только бинарник OrbitFM: `status`, `status --json`, `playpause`, `panel`,
`volume-up`, `volume-down`. Путь к бинарнику подставлен при установке.

При обновлении OrbitFM переустановите интеграцию: квитанция в
`$XDG_DATA_HOME/orbitfm/integrations/<цель>.json` помнит версию и хеши
файлов. Если файл изменён вручную, установщик откажется его перезаписывать
без `--force`. Удаление — `orbitfm integration uninstall <цель>`.
