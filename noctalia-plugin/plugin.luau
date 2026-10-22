--!nonstrict

barWidget.setGlyph("radio")
barWidget.setText(noctalia.tr("widget.label"))
barWidget.setTooltip(noctalia.tr("widget.tooltip"))

function onClick()
    local started = noctalia.runAsync("orbitfm toggle", function(result)
        if result.exitCode ~= 0 then
            local detail = result.stderr ~= "" and result.stderr or noctalia.tr("widget.error_body")
            noctalia.notify(noctalia.tr("widget.error_title"), detail)
        end
    end)

    if not started then
        noctalia.notify(noctalia.tr("widget.error_title"), noctalia.tr("widget.error_body"))
    end
end
