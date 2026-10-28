--!nonstrict

barWidget.setGlyph("radio")
barWidget.setText(noctalia.tr("widget.label"))
barWidget.setTooltip(noctalia.tr("widget.tooltip"))

function onClick()
    -- Клик по бар-виджету открывает панель виджета (ADR-0033): стеклянную
    -- поверхность `panel` со списком станций, громкостью и лентой эфира.
    -- Повторный клик скрывает её (команда `panel`). Полное окно Focus — по
    -- ярлыку из дока или с рабочего стола (`orbitfm mode focus`), и кнопкой
    -- «⤢» в шапке этой панели.
    -- Отсоединённый запуск: вариант runAsync с колбэком имеет таймаут 5 с
    -- и убивает группу процессов, поэтому GUI запускается без callback.
    local started = noctalia.runAsync("orbitfm panel")

    if not started then
        noctalia.notify(noctalia.tr("widget.error_title"), noctalia.tr("widget.error_body"))
    end
end
