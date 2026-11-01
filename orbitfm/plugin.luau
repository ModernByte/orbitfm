--!nonstrict

local DISPLAY = noctalia.getConfig("display") or "icon_text"
local STATE_COLORS = {
    playing = "#22C55E",
    buffering = "#22C55E",
    reconnecting = "#6B7280",
    error = "#EF4444",
}
local STATE_GLYPHS = {
    reconnecting = "radio-off",
}

local state = "stopped"
local pending = false

local function render()
    barWidget.setGlyph(STATE_GLYPHS[state] or "radio")
    barWidget.setGlyphColor(STATE_COLORS[state] or "on_surface")
    barWidget.setText(DISPLAY == "icon_text" and noctalia.tr("widget.label") or "")
    barWidget.setTooltip(noctalia.tr("widget.tooltip") .. " — " .. noctalia.tr("widget.state." .. state))
end

local function apply_status(result)
    if not result or result.exitCode ~= 0 then
        state = "stopped"
    else
        local reported = string.match(result.stdout, "^status%s+(%a+)")
        if reported == "playing" or reported == "buffering" or reported == "paused"
            or reported == "reconnecting" or reported == "error" then
            state = reported
        else
            state = "stopped"
        end
    end
    render()
end

function update()
    noctalia.setUpdateInterval(2000)
    if pending then
        return
    end
    pending = true
    noctalia.runAsync("orbitfm status", function(result)
        pending = false
        apply_status(result)
    end)
end

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

render()
