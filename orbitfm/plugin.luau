--!nonstrict

barWidget.setGlyph("radio")
barWidget.setText(noctalia.tr("widget.label"))
barWidget.setTooltip(noctalia.tr("widget.tooltip"))

function onClick()
    -- Отсоединённый запуск: вариант runAsync с колбэком имеет таймаут 5 с
    -- и убивает группу процессов, поэтому GUI запускается без callback.
    local started = noctalia.runAsync("orbitfm toggle")

    if not started then
        noctalia.notify(noctalia.tr("widget.error_title"), noctalia.tr("widget.error_body"))
    end
end
