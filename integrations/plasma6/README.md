# OrbitFM для Plasma 6

Виджет панели Plasma 6. Установка:

```
orbitfm integration install plasma6
kbuildsycoca6 --noincremental
```

Затем ПКМ по панели → «Добавить виджеты» → OrbitFM. На Wayland при
необходимости перезапустите Plasma.

## Управление

- ЛКМ — пуск/пауза.
- ПКМ — выпадающая панель OrbitFM (`orbitfm panel`).
- Колесо — громкость.
- Полная карточка виджета — кнопки «Пуск/Пауза», «Стоп» и «Настройки».

Подписи берутся из `orbitfm status --json` (`ui`): виджет не дублирует
каталог переводов и говорит на языке приложения.

## Как это работает

Виджет опрашивает `orbitfm status --json` через
`Plasma5Support.DataSource` (`executable`) раз в 2 секунды и отправляет
команды тем же бинарником. Своего состояния виджет не хранит. Если OrbitFM
не запущен, виджет показывает «OrbitFM» и не создаёт второй процесс.

Виджет не меняет конфиги Plasma. Удаление:

```
orbitfm integration uninstall plasma6
```
