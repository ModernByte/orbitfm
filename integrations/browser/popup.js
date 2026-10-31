"use strict";

const HOST = "org.modernbyte.orbitfm";
const api = globalThis.browser ?? globalThis.chrome;

const stateEl = document.getElementById("state");
const stationEl = document.getElementById("station");
const volumeEl = document.getElementById("volume");
const volumeLabel = document.getElementById("volume-label");
const themeEl = document.getElementById("theme");
const messageEl = document.getElementById("message");
const retryEl = document.getElementById("retry");
const windowButton = document.getElementById("window");
const playpauseButton = document.getElementById("playpause");
const previousButton = document.getElementById("previous");
const nextButton = document.getElementById("next");
const stopButton = document.getElementById("stop");
const rouletteButton = document.getElementById("roulette");
const settingsButton = document.getElementById("settings");

let port = null;
let nextId = 1;
let draggingVolume = false;

function disconnectPort() {
  const current = port;
  port = null;
  if (current) {
    try {
      current.disconnect();
    } catch (_error) {
      // Порт уже закрыт браузером.
    }
  }
}

function setMessage(text) {
  messageEl.textContent = text ?? "";
  messageEl.hidden = !text;
}

function setState(text, kind) {
  stateEl.textContent = text;
  stateEl.dataset.kind = kind;
}

function setControlsEnabled(enabled) {
  for (const button of document.querySelectorAll(".controls button, .row button")) {
    button.disabled = !enabled;
  }
  themeEl.disabled = !enabled;
  volumeEl.disabled = !enabled;
}

function setOffline(text) {
  disconnectPort();
  setState("offline", "offline");
  stationEl.textContent = "\u2014";
  setControlsEnabled(false);
  retryEl.hidden = false;
  setMessage(text);
}

function connect() {
  if (port) {
    return;
  }
  retryEl.hidden = true;
  setMessage("");
  setState("connecting", "connecting");
  if (typeof api?.runtime?.connectNative !== "function") {
    setOffline("Native messaging is unavailable in this browser.");
    return;
  }
  try {
    port = api.runtime.connectNative(HOST);
  } catch (error) {
    setOffline(String(error?.message ?? error));
    return;
  }
  port.onMessage.addListener(onMessage);
  port.onDisconnect.addListener(onDisconnect);
  send("events");
}

function onDisconnect() {
  const detail = api.runtime?.lastError?.message;
  setOffline(detail ? `OrbitFM is not available: ${detail}` : "OrbitFM is not running.");
}

function onMessage(message) {
  if (!message || typeof message !== "object") {
    return;
  }
  if (Object.prototype.hasOwnProperty.call(message, "event")) {
    const event = message.event;
    if (event === null) {
      setOffline(message.error ?? "The event stream has closed.");
      return;
    }
    if (message.ok === false || !event || typeof event !== "object") {
      return;
    }
    if (event.ev !== 1) {
      return;
    }
    if (event.type === "keepalive" || !event.status) {
      return;
    }
    render(event.status);
    return;
  }
  if (message.ok === false) {
    if (message.code === "not-running") {
      setOffline("OrbitFM is not running.");
      return;
    }
    if (message.stream) {
      setOffline(message.error ?? "The event stream has closed.");
      return;
    }
    setMessage(message.error ?? "OrbitFM rejected the command.");
  }
}

function send(command) {
  if (!port) {
    return;
  }
  port.postMessage({ id: nextId, command });
  nextId += 1;
}

function render(next) {
  if (!next || typeof next !== "object") {
    return;
  }
  setControlsEnabled(true);
  setMessage("");
  setState(String(next.state ?? "unknown"), String(next.state ?? "unknown"));
  const ui = next.ui ?? {};
  const station = next.station?.name;
  stationEl.textContent = station || ui.nothing_playing || "Nothing is playing";
  document.title = station ? `${station} \u2014 OrbitFM` : "OrbitFM";
  playpauseButton.textContent = ui.play_pause ?? "Play/Pause";
  stopButton.textContent = ui.stop ?? "Stop";
  previousButton.textContent = ui.previous ?? "Previous";
  nextButton.textContent = ui.next ?? "Next";
  rouletteButton.textContent = ui.radio_roulette ?? "Roulette";
  settingsButton.textContent = ui.settings ?? "Settings";
  const visible = Boolean(next.window?.visible);
  windowButton.textContent = visible ? (ui.hide_window ?? "Hide") : (ui.show_window ?? "Show");
  windowButton.dataset.command = visible ? "hide" : "show";
  previousButton.disabled = !next.can?.previous;
  nextButton.disabled = !next.can?.next;
  if (!draggingVolume) {
    volumeEl.value = String(Math.round((Number(next.volume) || 0) * 100));
    updateVolumeLabel();
  }
  themeEl.value = next.theme ?? "system";
}

function updateVolumeLabel() {
  volumeLabel.textContent = `Volume ${volumeEl.value}%`;
}

document.querySelectorAll("button[data-command]").forEach((button) => {
  button.addEventListener("click", () => send(button.dataset.command));
});

volumeEl.addEventListener("pointerdown", () => {
  draggingVolume = true;
});
volumeEl.addEventListener("pointerup", () => {
  draggingVolume = false;
});
volumeEl.addEventListener("pointercancel", () => {
  draggingVolume = false;
});
volumeEl.addEventListener("input", updateVolumeLabel);
volumeEl.addEventListener("change", () => {
  draggingVolume = false;
  send(`volume ${(Number(volumeEl.value) / 100).toFixed(2)}`);
});
themeEl.addEventListener("change", () => send(`theme ${themeEl.value}`));
retryEl.addEventListener("click", connect);

setControlsEnabled(false);
updateVolumeLabel();
connect();
