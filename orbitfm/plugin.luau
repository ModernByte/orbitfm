--!nonstrict

barWidget.setGlyph("radio")
barWidget.setText(noctalia.tr("widget.label"))
barWidget.setTooltip(noctalia.tr("widget.tooltip"))

function onClick()
    -- Клик по бар-виджету открывает капсулу Overlay (ADR-0033): полноценное
    -- окно без рамки вместо выпадающей панели; повторный клик скрывает её
    -- (команда `overlay`). Выпадающая панель остаётся CLI-поверхностью `panel`.
    -- Отсоединённый запуск: вариант runAsync с колбэком имеет таймаут 5 с
    -- и убивает группу процессов, поэтому GUI запускается без callback.
    local started = noctalia.runAsync("orbitfm overlay")

    if not started then
        noctalia.notify(noctalia.tr("widget.error_title"), noctalia.tr("widget.error_body"))
    end
end
