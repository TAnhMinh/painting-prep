// In-app debug panel (spec §7, D6): there is no Safari Web Inspector without
// a Mac, so the app shows its own console, errors, sizes and timings.
// Opened by a long-press on the title.
//
// Import this module first, so it captures messages logged while the other
// modules load.

const MAX_ENTRIES = 300;
const LONG_PRESS_MS = 600;

/** Captured log entries, oldest first: { time, level, text }. */
const entries = [];
let errorCount = 0;
let infoProvider = () => ({});

function format(arg) {
  if (arg instanceof Error) return arg.stack ? `${arg}\n${arg.stack}` : String(arg);
  if (typeof arg === "string") return arg;
  try {
    return JSON.stringify(arg, (_, v) => (typeof v === "number" ? Math.round(v * 100) / 100 : v));
  } catch {
    return String(arg);
  }
}

function record(level, args) {
  if (level === "error") errorCount++;
  entries.push({ time: performance.now(), level, text: args.map(format).join(" ") });
  if (entries.length > MAX_ENTRIES) entries.shift();
  if (panel && !panel.hidden) renderPanel();
}

for (const level of ["log", "info", "warn", "error"]) {
  const original = console[level].bind(console);
  console[level] = (...args) => {
    record(level, args);
    original(...args);
  };
}

window.addEventListener("error", (e) => {
  record("error", [`uncaught: ${e.message} (${e.filename}:${e.lineno}:${e.colno})`]);
});
window.addEventListener("unhandledrejection", (e) => {
  record("error", ["unhandled rejection:", e.reason]);
});

// Read by the WebKit smoke test.
window.prepDebug = {
  get errorCount() {
    return errorCount;
  },
  entries,
};

/** Registers a function returning `{ label: value }` rows for the panel. */
export function setInfoProvider(fn) {
  infoProvider = fn;
}

let panel = null;

function buildPanel() {
  panel = document.createElement("div");
  panel.id = "debug-panel";
  panel.className = "debug-panel";
  panel.hidden = true;
  panel.innerHTML = `
    <div class="debug-head">
      <strong>Debug</strong>
      <button type="button" data-action="copy">Copy</button>
      <button type="button" data-action="clear">Clear</button>
      <button type="button" data-action="close">Close</button>
    </div>
    <table class="debug-info"></table>
    <ol class="debug-log"></ol>`;
  panel.addEventListener("click", async (e) => {
    const action = e.target.dataset?.action;
    if (action === "close") panel.hidden = true;
    if (action === "clear") {
      entries.length = 0;
      errorCount = 0;
      renderPanel();
    }
    if (action === "copy") {
      try {
        await navigator.clipboard.writeText(panelText());
        e.target.textContent = "Copied";
      } catch {
        e.target.textContent = "No clipboard"; // needs HTTPS on iOS
      }
      setTimeout(() => (e.target.textContent = "Copy"), 1500);
    }
  });
  document.body.append(panel);
}

function infoRows() {
  const rows = {
    userAgent: navigator.userAgent,
    devicePixelRatio: window.devicePixelRatio,
    viewport: `${window.innerWidth} × ${window.innerHeight}`,
  };
  try {
    Object.assign(rows, infoProvider());
  } catch (e) {
    rows.infoError = String(e);
  }
  return rows;
}

function panelText() {
  const info = Object.entries(infoRows()).map(([k, v]) => `${k}: ${v}`);
  const log = entries.map((e) => `${(e.time / 1000).toFixed(2)} ${e.level}: ${e.text}`);
  return [...info, "", ...log].join("\n");
}

function renderPanel() {
  const table = panel.querySelector(".debug-info");
  table.replaceChildren(
    ...Object.entries(infoRows()).map(([k, v]) => {
      const tr = document.createElement("tr");
      const th = document.createElement("th");
      const td = document.createElement("td");
      th.textContent = k;
      td.textContent = v;
      tr.append(th, td);
      return tr;
    }),
  );
  const list = panel.querySelector(".debug-log");
  list.replaceChildren(
    ...entries.map((e) => {
      const li = document.createElement("li");
      li.className = e.level;
      li.textContent = `${(e.time / 1000).toFixed(2)} ${e.text}`;
      return li;
    }),
  );
}

export function openDebugPanel() {
  if (!panel) buildPanel();
  renderPanel();
  panel.hidden = false;
}

/** Opens the panel after holding `target` for LONG_PRESS_MS. */
export function bindLongPress(target) {
  let timer = null;
  const cancel = () => {
    clearTimeout(timer);
    timer = null;
  };
  target.addEventListener("pointerdown", () => {
    cancel();
    timer = setTimeout(openDebugPanel, LONG_PRESS_MS);
  });
  for (const type of ["pointerup", "pointercancel", "pointerleave"]) {
    target.addEventListener(type, cancel);
  }
  target.addEventListener("contextmenu", (e) => e.preventDefault());
}
