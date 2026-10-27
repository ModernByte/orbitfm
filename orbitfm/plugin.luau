--!nonstrict

barWidget.setGlyph("radio")
barWidget.setText(noctalia.tr("widget.label"))
barWidget.setTooltip(noctalia.tr("widget.tooltip"))

function onClick()
    -- Панель из бара — выпадающая поверхность без полей окна (ADR-0033);
    -- повторный клик по виджету скрывает её (команда `panel`).
    -- Отсоединённый запуск: вариант runAsync с колбэком имеет таймаут 5 с
    -- и убивает группу процессов, поэтому GUI запускается без callback.
    local started = noctalia.runAsync("orbitfm panel")

    if not started then
        noctalia.notify(noctalia.tr("widget.error_title"), noctalia.tr("widget.error_body"))
    end
end
