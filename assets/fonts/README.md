# Встроенные fallback-шрифты

Набор гарантирует читаемый интерфейс на системе без пакетов Noto CJK, Arabic
и Color Emoji (ADR-0041). Шрифты не заменяют основной `Ubuntu-Light`: они
подключаются в `Proportional`/`Monospace` с `FontPriority::Lowest` и
используются только для отсутствующих глифов. Деванагари (хинди) в набор не
входит и по-прежнему берётся из системного шрифта.

| Файл | Назначение | Размер |
|------|------------|--------|
| `NotoSans-Regular.ttf` | латиница, кириллица, греческий, пунктуация | 607 КиБ |
| `NotoSansArabic-Regular.ttf` | арабская вязь | 230 КиБ |
| `NotoSansCJK-Regular-subset.otf` | иероглифы каталога UI, кана, хангыль | ~1,8 МиБ |
| `NotoColorEmoji-flags-subset.ttf` | только региональные индикаторы флагов | 852 КиБ |

Суммарный бюджет — не более 6 МиБ (проверяется тестом `ui::fonts`).

## Лицензия

Все файлы распространяются по SIL Open Font License 1.1; полный текст —
`LICENSE-OFL.txt`. Copyright:

- Noto Sans / Noto Sans Arabic — © The Noto Project Authors;
- Noto Sans CJK — © 2014–2021 Adobe (http://www.adobe.com/);
- Noto Color Emoji — © Google Inc.

## Воспроизведение сабсетов

`ui-chars.txt` — все не-ASCII символы, встречающиеся в `src/**/*.rs` на момент
сборки сабсета (17 языков каталога). После добавления новых строк каталога
файл нужно обновить и пересобрать сабсет.

```sh
python3 -m fontTools.subset /usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc \
  --font-number=0 \
  --text-file=assets/fonts/ui-chars.txt \
  --unicodes='U+3000-303F,U+3040-30FF,U+FF00-FFEF,U+AC00-D7A3' \
  --layout-features='*' \
  --output-file=assets/fonts/NotoSansCJK-Regular-subset.otf

python3 -m fontTools.subset /usr/share/fonts/noto/NotoColorEmoji.ttf \
  --unicodes='U+1F1E6-1F1FF' \
  --no-hinting --desubroutinize \
  --output-file=assets/fonts/NotoColorEmoji-flags-subset.ttf
```

Латинский и арабский файлы копируются из дистрибутива без изменений. Полный
Noto CJK (5,9–11 МиБ) намеренно не бандлится: произвольные названия станций
вне `ui-chars.txt` остаются зависимостью системы.
