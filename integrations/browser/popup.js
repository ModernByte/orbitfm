"use strict";

const HOST = "org.modernbyte.orbitfm";
const api = globalThis.browser ?? globalThis.chrome;

const SEARCH_DEBOUNCE_MS = 300;
const SEARCH_TIMEOUT_MS = 15000;
const REQUEST_TIMEOUT_MS = 6000;
const PLAY_URL_TIMEOUT_MS = 12000;
const EVENT_SILENCE_MS = 20000;
const RECONNECT_DELAYS_MS = [1000, 2000, 4000, 8000, 10000];
const MAX_RECONNECTS = 6;
const MAX_CANDIDATES = 20;
const MAX_URL_LENGTH = 2048;
const MAX_QUERY_LENGTH = 128;
const THEME_MODES = ["system", "dark", "light"];

const ACCENTS = {
  blue: [108, 176, 255],
  green: [104, 211, 145],
  amber: [255, 190, 92],
  violet: [178, 148, 255],
  rose: [255, 138, 165],
  cyan: [92, 214, 224],
};

const $ = (id) => document.getElementById(id);

const els = {
  root: document.documentElement,
  panel: document.querySelector(".panel"),
  statePill: $("state-pill"),
  state: $("state"),
  monogram: $("monogram"),
  nowLabel: $("now-label"),
  sourceBadge: $("source-badge"),
  station: $("station"),
  meta: $("meta"),
  favorite: $("favorite"),
  previous: $("previous"),
  playpause: $("playpause"),
  stop: $("stop"),
  next: $("next"),
  roulette: $("roulette"),
  window: $("window"),
  theme: $("theme"),
  settings: $("settings"),
  volume: $("volume"),
  volumeLabel: $("volume-label"),
  volumeValue: $("volume-value"),
  volumeDown: $("volume-down"),
  volumeUp: $("volume-up"),
  tabs: Array.from(document.querySelectorAll(".tab")),
  panels: {
    search: $("panel-search"),
    favorites: $("panel-favorites"),
    history: $("panel-history"),
  },
  query: $("query"),
  spinner: $("search-spinner"),
  searchNote: $("search-note"),
  searchNoteText: $("search-note-text"),
  searchRetry: $("search-retry"),
  searchResults: $("search-results"),
  favoritesList: $("favorites"),
  historyList: $("history"),
  picker: $("page-picker"),
  pickerTitle: $("picker-title"),
  pickerClose: $("picker-close"),
  pageCandidates: $("page-candidates"),
  page: $("page"),
  pageLabel: $("page-label"),
  retry: $("retry"),
  message: $("message"),
};

let port = null;
let nextRequestId = 1;
const pending = new Map();

let connected = false;
let currentStatus = null;
let labels = {};
let library = { favorites: [], history: [] };
let activeTab = "search";
let pageBusy = false;
let draggingVolume = false;

let reconnectAttempt = 0;
let reconnectTimer = 0;
let eventWatchdog = 0;

let queryTimer = 0;
const search = { sequence: 0, generation: null, timer: 0, query: "", earlyFrame: null };

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function setMessage(text, kind = "error") {
  els.message.textContent = text ?? "";
  els.message.dataset.kind = kind;
  els.message.hidden = !text;
}

function stateLabel(kind) {
  return labels[`state_${kind}`] ?? kind;
}

function setState(kind) {
  els.state.textContent = stateLabel(kind);
  els.statePill.dataset.kind = kind;
  els.root.dataset.state = kind;
}

function iconUse(symbol) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", `#${symbol}`);
  svg.append(use);
  return svg;
}

/* ------------------------------------------------------------------ */
/* Соединение и состояние                                             */
/* ------------------------------------------------------------------ */

function request(command, handlers = {}) {
  if (!port) {
    const error = "OrbitFM is not connected.";
    if (handlers.fail) {
      handlers.fail({ error });
    } else {
      setMessage(error);
    }
    return null;
  }

  const id = nextRequestId;
  nextRequestId += 1;
  const entry = { done: handlers.done, fail: handlers.fail, timer: 0 };
  const timeout = handlers.timeout ?? REQUEST_TIMEOUT_MS;
  if (timeout > 0) {
    entry.timer = setTimeout(() => {
      if (!pending.has(id)) {
        return;
      }
      pending.delete(id);
      const message = { error: "The command timed out." };
      if (handlers.fail) {
        handlers.fail(message);
      } else {
        setMessage(message.error);
      }
    }, timeout);
  }
  pending.set(id, entry);

  try {
    port.postMessage({ id, command });
  } catch (error) {
    pending.delete(id);
    clearTimeout(entry.timer);
    const message = String(error?.message ?? error);
    if (handlers.fail) {
      handlers.fail({ error: message });
    } else {
      setMessage(message);
    }
    return null;
  }
  return id;
}

function defaultFailure(message) {
  if (message.code === "not-running") {
    failOffline("OrbitFM is not running.");
    return;
  }
  if (message.stream) {
    failOffline(message.error ?? "The event stream has closed.");
    return;
  }
  setMessage(message.error ?? "OrbitFM rejected the command.");
}

function settleReply(message) {
  const entry = message.id === undefined ? undefined : pending.get(message.id);
  if (entry) {
    pending.delete(message.id);
    clearTimeout(entry.timer);
    if (message.ok === false) {
      if (entry.fail) {
        entry.fail(message);
      } else {
        defaultFailure(message);
      }
    } else if (entry.done) {
      const response = String(message.response ?? "");
      // IPC считает ответ ошибочным, если после пробелов идёт `error …`;
      // native host передаёт такую строку в успешном конверте как есть.
      if (/^\s*error\b/.test(response) && entry.fail) {
        entry.fail({ error: response.replace(/^\s*error\s*/, "") });
      } else {
        entry.done(response);
      }
    }
    return;
  }
  if (message.ok === false) {
    defaultFailure(message);
  }
}

function armEventWatchdog() {
  clearTimeout(eventWatchdog);
  eventWatchdog = setTimeout(() => {
    if (port) {
      failOffline("The event stream went silent.");
    }
  }, EVENT_SILENCE_MS);
}

function onMessage(message) {
  if (!message || typeof message !== "object") {
    return;
  }

  if (Object.prototype.hasOwnProperty.call(message, "event")) {
    armEventWatchdog();
    connected = true;
    const event = message.event;
    if (event === null) {
      failOffline(message.error ?? "The event stream has closed.");
      return;
    }
    if (message.ok === false || !event || typeof event !== "object" || event.ev !== 1) {
      return;
    }
    if (event.type === "search") {
      onSearchFrame(event);
      return;
    }
    if (event.type === "keepalive") {
      return;
    }
    if (event.status) {
      render(event.status);
    }
    return;
  }

  if (message.id !== undefined) {
    settleReply(message);
    return;
  }
  if (message.ok === false) {
    defaultFailure(message);
  }
}

function onDisconnect() {
  const detail = api.runtime?.lastError?.message;
  const wasActive = port !== null;
  port = null;
  connected = false;
  if (!wasActive) {
    return;
  }
  failOffline(detail ? `OrbitFM is not running: ${detail}` : "OrbitFM is not running.");
}

function disconnectPort() {
  const current = port;
  port = null;
  connected = false;
  clearTimeout(eventWatchdog);
  eventWatchdog = 0;
  for (const entry of pending.values()) {
    clearTimeout(entry.timer);
  }
  pending.clear();
  if (current) {
    try {
      current.disconnect();
    } catch (_error) {
      // Порт уже закрыт браузером.
    }
  }
}

function scheduleReconnect() {
  if (reconnectAttempt >= MAX_RECONNECTS) {
    return;
  }
  const delay = RECONNECT_DELAYS_MS[Math.min(reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
  reconnectAttempt += 1;
  clearTimeout(reconnectTimer);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = 0;
    connect();
  }, delay);
}

function failOffline(text) {
  disconnectPort();
  setState("offline");
  els.playpause.dataset.playing = "0";
  els.retry.hidden = false;
  setMessage(text);
  resetSearchUi();
  updateControls();
  scheduleReconnect();
}

function connect() {
  if (port) {
    return;
  }
  clearTimeout(reconnectTimer);
  reconnectTimer = 0;
  setState("connecting");
  setMessage("");

  if (typeof api?.runtime?.connectNative !== "function") {
    failOffline("Native messaging is unavailable in this browser.");
    return;
  }

  try {
    port = api.runtime.connectNative(HOST);
  } catch (error) {
    port = null;
    failOffline(String(error?.message ?? error));
    return;
  }

  port.onMessage.addListener(onMessage);
  port.onDisconnect.addListener(onDisconnect);

  request("events", {
    timeout: REQUEST_TIMEOUT_MS,
    done: () => {
      connected = true;
      reconnectAttempt = 0;
      els.retry.hidden = true;
      setMessage("");
      armEventWatchdog();
      updateControls();
      refreshLibrary();
      if (search.query && els.query.value.trim()) {
        runSearch(els.query.value);
      }
    },
    fail: (message) => failOffline(message.error ?? "The event stream has closed."),
  });
}

/* ------------------------------------------------------------------ */
/* Тема, акцент и подписи                                             */
/* ------------------------------------------------------------------ */

function resolvedTheme() {
  const mode = currentStatus?.theme;
  if (mode === "dark" || mode === "light") {
    return mode;
  }
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function applyAccent(token) {
  const rgb = ACCENTS[token] ?? ACCENTS.blue;
  const accent =
    resolvedTheme() === "light"
      ? rgb.map((channel) => Math.round(channel * 0.6))
      : rgb;
  els.root.style.setProperty("--accent-marker", `rgb(${rgb.join(", ")})`);
  els.root.style.setProperty("--accent-rgb", accent.join(", "));
}

function applyTheme(mode) {
  if (mode === "dark" || mode === "light") {
    els.root.dataset.theme = mode;
  } else {
    delete els.root.dataset.theme;
  }
  applyAccent(currentStatus?.accent ?? "blue");
}

function updateThemeButton() {
  const mode = els.theme.dataset.mode || "system";
  const modeName =
    mode === "dark"
      ? (labels.theme_dark ?? "Dark")
      : mode === "light"
        ? (labels.theme_light ?? "Light")
        : (labels.theme_system ?? "System");
  const label = `${labels.theme ?? "Theme"}: ${modeName}`;
  els.theme.setAttribute("aria-label", label);
  els.theme.title = label;
}

function applyLabels() {
  els.nowLabel.textContent = labels.now_playing ?? "Now playing";
  els.query.placeholder = labels.search ?? "Search stations";
  els.query.setAttribute("aria-label", labels.search ?? "Search stations");
  els.volumeLabel.textContent = labels.volume ?? "Volume";
  els.volume.setAttribute("aria-label", labels.volume ?? "Volume");
  els.volumeDown.setAttribute("aria-label", labels.volume_down ?? "Volume down");
  els.volumeDown.title = labels.volume_down ?? "Volume down";
  els.volumeUp.setAttribute("aria-label", labels.volume_up ?? "Volume up");
  els.volumeUp.title = labels.volume_up ?? "Volume up";

  els.previous.setAttribute("aria-label", labels.previous ?? "Previous");
  els.previous.title = labels.previous ?? "Previous";
  els.next.setAttribute("aria-label", labels.next ?? "Next");
  els.next.title = labels.next ?? "Next";
  els.playpause.setAttribute("aria-label", labels.play_pause ?? "Play/Pause");
  els.playpause.title = labels.play_pause ?? "Play/Pause";
  els.stop.setAttribute("aria-label", labels.stop ?? "Stop");
  els.stop.title = labels.stop ?? "Stop";
  els.roulette.setAttribute("aria-label", labels.radio_roulette ?? "Roulette");
  els.roulette.title = labels.radio_roulette ?? "Roulette";
  els.settings.setAttribute("aria-label", labels.settings ?? "Settings");
  els.settings.title = labels.settings ?? "Settings";
  els.window.setAttribute("aria-label", labels.show_window ?? "Show window");
  els.window.title = labels.show_window ?? "Show window";

  const tabLabels = {
    search: labels.search ?? "Search",
    favorites: labels.favorites ?? "Favorites",
    history: labels.history ?? "History",
  };
  for (const tab of els.tabs) {
    tab.textContent = tabLabels[tab.dataset.tab] ?? tab.dataset.tab;
  }

  els.pageLabel.textContent = labels.play_this_page ?? "Play this page";
  els.pickerTitle.textContent = labels.play_this_page ?? "Play this page";
  els.pickerClose.setAttribute("aria-label", labels.close ?? "Close");
  els.pickerClose.title = labels.close ?? "Close";
  els.retry.textContent = labels.retry ?? "Reconnect";
  els.searchRetry.textContent = labels.retry ?? "Retry";

  updateThemeButton();
  renderFavorite(Boolean(currentStatus?.station?.favorite));
}

/* ------------------------------------------------------------------ */
/* Отрисовка статуса                                                  */
/* ------------------------------------------------------------------ */

function initial(value) {
  const first = Array.from(value.trim())[0];
  return first ? first.toUpperCase() : "◍";
}

function hashText(value) {
  let hash = 2166136261;
  for (const char of value) {
    hash ^= char.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16777619) >>> 0;
  }
  return hash >>> 0;
}

function monogramGradient(name) {
  const hue = hashText(name) % 360;
  const second = (hue + 42) % 360;
  return `linear-gradient(135deg, hsl(${hue}, 68%, 56%), hsl(${second}, 70%, 46%))`;
}

function stationMeta(station) {
  const tags = String(station.tags ?? "")
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean)
    .slice(0, 2)
    .join(", ");
  return [
    station.country || station.countrycode || "",
    tags,
    station.bitrate ? `${station.bitrate} kbps` : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

function renderFavorite(favorite) {
  const active = Boolean(favorite);
  els.favorite.classList.toggle("is-favorite", active);
  els.favorite.setAttribute("aria-pressed", active ? "true" : "false");
  const label = active
    ? (labels.remove_favorite ?? "Remove favorite")
    : (labels.add_favorite ?? "Favorite");
  els.favorite.setAttribute("aria-label", label);
  els.favorite.title = label;
}

function renderStation(station) {
  if (station && station.name) {
    els.station.textContent = station.name;
    const meta = stationMeta(station);
    els.meta.textContent = meta;
    els.meta.hidden = !meta;
    els.sourceBadge.hidden = station.source !== "stream";
    els.monogram.textContent = initial(station.name);
    els.monogram.classList.remove("is-empty");
    els.monogram.style.background = monogramGradient(station.name);
    document.title = `${station.name} — OrbitFM`;
    els.favorite.hidden = false;
    renderFavorite(station.favorite);
    return;
  }

  els.station.textContent = labels.nothing_playing ?? "Nothing is playing";
  els.meta.hidden = true;
  els.sourceBadge.hidden = true;
  els.monogram.textContent = "◍";
  els.monogram.classList.add("is-empty");
  els.monogram.style.removeProperty("background");
  document.title = "OrbitFM";
  els.favorite.hidden = true;
  els.favorite.classList.remove("is-favorite");
  els.favorite.setAttribute("aria-pressed", "false");
}

function syncVolume(volume) {
  const percent = clamp(Math.round((Number(volume) || 0) * 100), 0, 100);
  els.volume.value = String(percent);
  els.volumeValue.textContent = `${percent}%`;
}

function render(next) {
  currentStatus = next;
  connected = true;

  if (next.ui && typeof next.ui === "object") {
    labels = next.ui;
  }
  if (typeof next.language === "string" && next.language) {
    els.root.lang = next.language;
  }

  applyLabels();
  applyTheme(next.theme);
  applyAccent(next.accent);

  const state = String(next.state ?? "unknown");
  setState(state);
  renderStation(next.station);

  els.playpause.dataset.playing = state === "playing" ? "1" : "0";

  const visible = Boolean(next.window?.visible);
  els.window.dataset.command = visible ? "hide" : "show";
  const windowLabel = visible
    ? (labels.hide_window ?? "Hide window")
    : (labels.show_window ?? "Show window");
  els.window.setAttribute("aria-label", windowLabel);
  els.window.title = windowLabel;
  els.window.setAttribute("aria-pressed", visible ? "true" : "false");

  const mode = THEME_MODES.includes(next.theme) ? next.theme : "system";
  els.theme.dataset.mode = mode;
  updateThemeButton();

  if (!draggingVolume) {
    syncVolume(next.volume);
  }
  updateControls();
}

function updateControls() {
  const online = connected && Boolean(currentStatus);
  const station = Boolean(currentStatus?.station);
  els.previous.disabled = !online || !currentStatus?.can?.previous;
  els.next.disabled = !online || !currentStatus?.can?.next;
  els.playpause.disabled = !online || !station;
  els.stop.disabled = !online || !station;
  els.roulette.disabled = !online;
  els.favorite.disabled = !online || !station;
  els.volume.disabled = !online;
  els.volumeDown.disabled = !online;
  els.volumeUp.disabled = !online;
  els.theme.disabled = !online;
  els.settings.disabled = !online;
  els.window.disabled = !online;
  els.page.disabled = !online || pageBusy;
  els.query.disabled = !online;
  els.searchRetry.disabled = !online;
}

/* ------------------------------------------------------------------ */
/* Поиск                                                              */
/* ------------------------------------------------------------------ */

function setSearchBusy(busy) {
  els.spinner.hidden = !busy;
  els.query.setAttribute("aria-busy", busy ? "true" : "false");
}

function showSearchNote(text, error, retryVisible) {
  els.searchNoteText.textContent = text ?? "";
  els.searchNote.classList.toggle("is-error", Boolean(error));
  els.searchRetry.hidden = !retryVisible;
  els.searchNote.hidden = !text;
}

function resetSearchUi() {
  search.sequence += 1;
  search.generation = null;
  search.earlyFrame = null;
  clearTimeout(search.timer);
  search.timer = 0;
  clearTimeout(queryTimer);
  setSearchBusy(false);
  showSearchNote("", false, false);
  els.searchResults.replaceChildren();
  els.searchResults.hidden = true;
}

function finishSearch(sequence, error) {
  if (sequence !== search.sequence) {
    return;
  }
  clearTimeout(search.timer);
  search.timer = 0;
  search.generation = null;
  search.earlyFrame = null;
  setSearchBusy(false);
  showSearchNote(error, true, true);
  els.searchResults.replaceChildren();
  els.searchResults.hidden = true;
}

function runSearch(rawQuery) {
  const query = String(rawQuery ?? "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, MAX_QUERY_LENGTH);
  if (!connected || !query) {
    return;
  }

  search.sequence += 1;
  const sequence = search.sequence;
  search.query = query;
  search.generation = null;
  search.earlyFrame = null;
  clearTimeout(search.timer);
  search.timer = 0;
  setSearchBusy(true);
  showSearchNote("", false, false);

  request(`search ${query}`, {
    timeout: REQUEST_TIMEOUT_MS,
    done: (response) => {
      if (sequence !== search.sequence) {
        return;
      }
      const match = /^search\s+(\d+)$/.exec(String(response ?? ""));
      if (!match) {
        finishSearch(sequence, "The search was rejected.");
        return;
      }
      search.generation = Number(match[1]);
      // Кадр с результатами может прийти раньше ответа `search <id>`:
      // сохраняем его и применяем, как только поколение стало известно.
      const early = search.earlyFrame;
      search.earlyFrame = null;
      if (early && Number(early.id) === search.generation) {
        onSearchFrame(early);
        return;
      }
      search.timer = setTimeout(() => {
        finishSearch(sequence, "The search timed out.");
      }, SEARCH_TIMEOUT_MS);
    },
    fail: (message) => {
      finishSearch(sequence, message.error ?? "The search failed.");
    },
  });
}

function onSearchFrame(frame) {
  if (!search.query || frame.query !== search.query) {
    return;
  }
  if (search.generation === null) {
    search.earlyFrame = frame;
    return;
  }
  if (Number(frame.id) !== search.generation) {
    return;
  }

  clearTimeout(search.timer);
  search.timer = 0;
  setSearchBusy(false);

  if (frame.error) {
    showSearchNote(String(frame.error), true, true);
    els.searchResults.replaceChildren();
    els.searchResults.hidden = true;
    return;
  }

  const results = Array.isArray(frame.results) ? frame.results : [];
  if (!results.length) {
    showSearchNote(labels.no_match ?? "Nothing found", false, false);
    els.searchResults.replaceChildren();
    els.searchResults.hidden = true;
    return;
  }

  showSearchNote("", false, false);
  const fragment = document.createDocumentFragment();
  for (const station of results.slice(0, MAX_CANDIDATES)) {
    fragment.append(stationItem(station, "result"));
  }
  els.searchResults.replaceChildren(fragment);
  els.searchResults.hidden = false;
}

/* ------------------------------------------------------------------ */
/* Библиотека и списки                                                */
/* ------------------------------------------------------------------ */

function stationItem(station, mode) {
  const item = document.createElement("li");
  const button = document.createElement("button");
  button.type = "button";
  button.className = "result";
  const name = String(station?.name ?? station?.uuid ?? "—");
  button.setAttribute("aria-label", `${name} — ${labels.play_pause ?? "Play"}`);

  const title = document.createElement("span");
  title.className = "result-name";
  title.textContent = name;

  const meta = document.createElement("span");
  meta.className = "result-meta";
  meta.textContent = stationMeta(station ?? {});

  const play = document.createElement("span");
  play.className = "result-play";
  play.setAttribute("aria-hidden", "true");
  play.append(iconUse("i-play"));

  button.append(title, meta, play);
  button.addEventListener("click", () => playStation(station, mode, button));
  item.append(button);
  return item;
}

function renderList(list, stations, mode) {
  const fragment = document.createDocumentFragment();
  if (!stations.length) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = "—";
    fragment.append(empty);
  } else {
    for (const station of stations.slice(0, MAX_CANDIDATES)) {
      fragment.append(stationItem(station, mode));
    }
  }
  list.replaceChildren(fragment);
}

function renderLibrary() {
  renderList(els.favoritesList, library.favorites, "library");
  renderList(els.historyList, library.history, "library");
}

function refreshLibrary() {
  if (!connected) {
    return;
  }
  request("library", {
    timeout: REQUEST_TIMEOUT_MS,
    done: (response) => {
      try {
        const data = JSON.parse(String(response ?? ""));
        library = {
          favorites: Array.isArray(data?.favorites) ? data.favorites : [],
          history: Array.isArray(data?.history) ? data.history : [],
        };
      } catch (_error) {
        library = { favorites: [], history: [] };
      }
      renderLibrary();
    },
    fail: () => {
      // Списки остаются прежними: сбой чтения библиотеки не должен их стирать.
    },
  });
}

function playStation(station, mode, button) {
  if (!connected || !station?.uuid) {
    return;
  }
  const command =
    mode === "library"
      ? `play-library ${station.uuid}`
      : `play-result ${station.uuid}`;
  button.disabled = true;
  setMessage("");
  request(command, {
    timeout: REQUEST_TIMEOUT_MS,
    done: () => {
      button.disabled = false;
      setMessage("");
      refreshLibrary();
    },
    fail: (message) => {
      button.disabled = false;
      setMessage(message.error ?? "OrbitFM rejected the station.");
    },
  });
}

/* ------------------------------------------------------------------ */
/* Вкладки                                                            */
/* ------------------------------------------------------------------ */

function selectTab(name, focus = false) {
  activeTab = name;
  for (const tab of els.tabs) {
    const selected = tab.dataset.tab === name;
    tab.setAttribute("aria-selected", selected ? "true" : "false");
    tab.tabIndex = selected ? 0 : -1;
  }
  for (const [key, panel] of Object.entries(els.panels)) {
    panel.hidden = key !== name;
  }
  if (name !== "search") {
    refreshLibrary();
  }
  if (focus) {
    if (name === "search") {
      els.query.focus({ preventScroll: true });
    } else {
      els.tabs.find((tab) => tab.dataset.tab === name)?.focus({ preventScroll: true });
    }
  }
}

function cycleTab(index, delta) {
  const nextIndex = (index + delta + els.tabs.length) % els.tabs.length;
  const nextTab = els.tabs[nextIndex];
  selectTab(nextTab.dataset.tab);
  nextTab.focus({ preventScroll: true });
}

/* ------------------------------------------------------------------ */
/* Stream Discovery: «Play this page»                                 */
/* ------------------------------------------------------------------ */

// Функция инжектится в активную вкладку через scripting.executeScript:
// замыканий нет, только document и стандартные DOM-API.
function scanDocument() {
  const audio = [];
  const push = (value) => {
    if (typeof value === "string" && value.trim() && audio.length < 20) {
      audio.push(value.trim());
    }
  };
  const meta = document.querySelector(
    'meta[property="og:audio"], meta[name="og:audio"], meta[property="og:audio:url"]',
  );
  if (meta) {
    push(meta.getAttribute("content"));
  }
  document.querySelectorAll("audio[src]").forEach((element) => {
    push(element.currentSrc || element.src);
  });
  document.querySelectorAll("audio source[src]").forEach((element) => {
    push(element.src);
  });
  return {
    title: typeof document.title === "string" ? document.title : "",
    audio,
  };
}

function describeUrl(value) {
  try {
    const parsed = new URL(value);
    const host = parsed.hostname.replace(/^www\./, "");
    const path = parsed.pathname === "/" ? "" : parsed.pathname;
    const text = `${host}${path}`;
    return text.length > 72 ? `${text.slice(0, 71)}…` : text;
  } catch (_error) {
    return value.slice(0, 72);
  }
}

async function collectPageCandidates() {
  let tab = null;
  try {
    const tabs = await api.tabs.query({ active: true, currentWindow: true });
    tab = Array.isArray(tabs) ? tabs[0] : null;
  } catch (_error) {
    tab = null;
  }

  const pageUrl = typeof tab?.url === "string" ? tab.url : "";
  const pageTitle = typeof tab?.title === "string" ? tab.title : "";
  const candidates = [];
  const seen = new Set();

  const add = (raw, title) => {
    if (typeof raw !== "string" || !raw) {
      return;
    }
    let url;
    try {
      url = new URL(raw, pageUrl || undefined).href;
    } catch (_error) {
      return;
    }
    if (!/^https?:\/\//i.test(url) || /\s/.test(url) || url.length > MAX_URL_LENGTH) {
      return;
    }
    if (seen.has(url)) {
      return;
    }
    seen.add(url);
    candidates.push({ url, title: title || "", meta: describeUrl(url) });
  };

  if (/^https?:\/\//i.test(pageUrl)) {
    add(pageUrl, pageTitle);
  }

  if (tab && Number.isInteger(tab.id) && typeof api?.scripting?.executeScript === "function") {
    try {
      const injections = await api.scripting.executeScript({
        target: { tabId: tab.id },
        func: scanDocument,
      });
      const found = Array.isArray(injections) ? injections[0]?.result : null;
      if (found && typeof found === "object" && Array.isArray(found.audio)) {
        const title = typeof found.title === "string" && found.title ? found.title : pageTitle;
        for (const audio of found.audio.slice(0, MAX_CANDIDATES)) {
          add(audio, title);
        }
      }
    } catch (_error) {
      // Служебные страницы браузера не сканируются: остаётся URL вкладки.
    }
  }

  return candidates.slice(0, MAX_CANDIDATES);
}

function hidePicker() {
  els.picker.hidden = true;
  els.panel.classList.remove("is-picking");
  els.page.focus({ preventScroll: true });
}

function showPicker(candidates) {
  const fragment = document.createDocumentFragment();
  for (const candidate of candidates) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "result";

    const title = document.createElement("span");
    title.className = "result-name";
    title.textContent = candidate.title || labels.play_this_page || "Play this page";
    button.setAttribute("aria-label", `${title.textContent} — ${labels.play_pause ?? "Play"}`);

    const meta = document.createElement("span");
    meta.className = "result-meta";
    meta.textContent = candidate.meta;

    const play = document.createElement("span");
    play.className = "result-play";
    play.setAttribute("aria-hidden", "true");
    play.append(iconUse("i-play"));

    button.append(title, meta, play);
    button.addEventListener("click", () => {
      hidePicker();
      playUrl(candidate.url);
    });
    item.append(button);
    fragment.append(item);
  }
  els.pageCandidates.replaceChildren(fragment);
  els.picker.hidden = false;
  els.panel.classList.add("is-picking");
  els.pageCandidates.querySelector("button")?.focus({ preventScroll: true });
}

function playUrl(url) {
  if (!connected) {
    return;
  }
  pageBusy = true;
  updateControls();
  setMessage(labels.importing ?? "Importing stream…", "info");
  request(`play-url ${url}`, {
    timeout: PLAY_URL_TIMEOUT_MS,
    done: () => {
      pageBusy = false;
      updateControls();
      setMessage("");
      refreshLibrary();
    },
    fail: (message) => {
      pageBusy = false;
      updateControls();
      setMessage(message.error ?? labels.stream_rejected ?? "The stream was rejected.");
    },
  });
}

async function onPlayThisPage() {
  if (!connected || pageBusy) {
    return;
  }
  pageBusy = true;
  updateControls();
  setMessage("");
  try {
    const candidates = await collectPageCandidates();
    if (!candidates.length) {
      setMessage(labels.no_audio ?? "No audio found on this page.");
      return;
    }
    showPicker(candidates);
  } finally {
    pageBusy = false;
    updateControls();
  }
}

/* ------------------------------------------------------------------ */
/* Обработчики                                                        */
/* ------------------------------------------------------------------ */

function sendCommand(command) {
  request(command, {
    done: () => setMessage(""),
    fail: (message) => setMessage(message.error ?? "OrbitFM rejected the command."),
  });
}

function nudgeVolume(delta) {
  if (!connected) {
    return;
  }
  const percent = clamp((Number(els.volume.value) || 0) + delta, 0, 100);
  els.volume.value = String(percent);
  els.volumeValue.textContent = `${percent}%`;
  sendCommand(`volume ${(percent / 100).toFixed(2)}`);
}

function cycleTheme() {
  const current = els.theme.dataset.mode || "system";
  const index = THEME_MODES.indexOf(current);
  const next = THEME_MODES[(index + 1) % THEME_MODES.length];
  els.theme.dataset.mode = next;
  updateThemeButton();
  applyTheme(next);
  sendCommand(`theme ${next}`);
}

function toggleFavorite() {
  if (!connected || !currentStatus?.station) {
    return;
  }
  request("favorite", {
    timeout: REQUEST_TIMEOUT_MS,
    done: (response) => {
      const match = /^favorite\s+(true|false)\b/.exec(String(response ?? ""));
      if (!match) {
        setMessage("The favorite was rejected.");
        return;
      }
      currentStatus.station.favorite = match[1] === "true";
      renderFavorite(currentStatus.station.favorite);
      setMessage("");
      refreshLibrary();
    },
    fail: (message) => setMessage(message.error ?? "The favorite was rejected."),
  });
}

function bind() {
  for (const button of document.querySelectorAll("button[data-command]")) {
    if (button.id === "volume-up" || button.id === "volume-down") {
      continue;
    }
    button.addEventListener("click", () => sendCommand(button.dataset.command));
  }

  els.volumeDown.addEventListener("click", () => nudgeVolume(-5));
  els.volumeUp.addEventListener("click", () => nudgeVolume(5));
  els.theme.addEventListener("click", cycleTheme);
  els.favorite.addEventListener("click", toggleFavorite);
  els.page.addEventListener("click", onPlayThisPage);
  els.pickerClose.addEventListener("click", hidePicker);

  els.tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => selectTab(tab.dataset.tab));
    tab.addEventListener("keydown", (event) => {
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        cycleTab(index, -1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        cycleTab(index, 1);
      }
    });
  });

  els.query.addEventListener("input", () => {
    clearTimeout(queryTimer);
    const query = els.query.value.trim();
    if (!query) {
      resetSearchUi();
      return;
    }
    queryTimer = setTimeout(() => runSearch(query), SEARCH_DEBOUNCE_MS);
  });
  els.query.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") {
      return;
    }
    event.preventDefault();
    clearTimeout(queryTimer);
    runSearch(els.query.value);
  });
  els.searchRetry.addEventListener("click", () => {
    if (search.query) {
      runSearch(search.query);
    }
  });

  els.volume.addEventListener("pointerdown", () => {
    draggingVolume = true;
  });
  for (const type of ["pointerup", "pointercancel"]) {
    window.addEventListener(type, () => {
      draggingVolume = false;
    });
  }
  els.volume.addEventListener("input", () => {
    els.volumeValue.textContent = `${Number(els.volume.value) || 0}%`;
  });
  els.volume.addEventListener("change", () => {
    if (!connected) {
      return;
    }
    const level = (Number(els.volume.value) || 0) / 100;
    sendCommand(`volume ${level.toFixed(2)}`);
  });
  window.addEventListener("keyup", (event) => {
    if (event.target === els.volume && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(event.key)) {
      draggingVolume = false;
    }
  });

  els.retry.addEventListener("click", () => {
    reconnectAttempt = 0;
    clearTimeout(reconnectTimer);
    reconnectTimer = 0;
    connect();
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !els.picker.hidden) {
      event.preventDefault();
      hidePicker();
    }
  });
}

/* ------------------------------------------------------------------ */
/* Старт                                                              */
/* ------------------------------------------------------------------ */

matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  applyAccent(currentStatus?.accent ?? "blue");
});

bind();
applyLabels();
applyTheme("system");
applyAccent("blue");
renderFavorite(false);
renderLibrary();
syncVolume(0);
selectTab("search");
updateControls();
connect();
