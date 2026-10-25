// What both windows share: the commands, the language, small drawing helpers.
const { invoke } = window.__TAURI__.core;
const { listen, emit } = window.__TAURI__.event;

// The pages are written in French; ENGLISH words them for an English system.
let ENGLISH = null;
const t = (text) => (ENGLISH && ENGLISH[text]) || text;
const $ = (id) => document.getElementById(id);
const escape = (text) => String(text).replace(/[&<>"']/g,
  (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

async function translate(english) {
  if ((await invoke("ui_lang")) === "en") ENGLISH = english;
  for (const node of document.querySelectorAll("[data-t]")) node.textContent = t(node.textContent.trim());
  for (const name of ["placeholder", "aria-label"]) {
    for (const node of document.querySelectorAll(`[${name}]`)) node.setAttribute(name, t(node.getAttribute(name)));
  }
}

/** French puts a space before ":" and ";", English does not. */
function colon() { return ENGLISH ? ": " : " : "; }
function semi() { return ENGLISH ? "; " : " ; "; }

/** Decimal sizes, as Delest's core writes them. */
function size(bytes) {
  const fr = !ENGLISH;
  const [value, unit] = bytes >= 1e9 ? [bytes / 1e9, fr ? "Go" : "GB"]
    : bytes >= 1e6 ? [bytes / 1e6, fr ? "Mo" : "MB"] : [bytes / 1e3, fr ? "ko" : "kB"];
  const text = value.toFixed(value < 10 && bytes >= 1e9 ? 1 : 0);
  return `${fr ? text.replace(".", ",") : text} ${unit}`;
}

/** Shows that a model is at work in `node`: LEDs, the text, the seconds elapsed. Returns what stops it. */
function thinking(node, text) {
  const start = Date.now();
  const leds = Array.from({ length: 8 }, (_, i) => `<i style="--i:${i}"></i>`).join("");
  const box = document.createElement("span");
  box.className = "thinking";
  box.innerHTML = `<span class="leds" aria-hidden="true">${leds}</span><span role="status">${escape(text)}</span><span class="clock" aria-hidden="true">0 s</span>`;
  node.append(box);
  const clock = box.querySelector(".clock");
  const timer = setInterval(() => { clock.textContent = `${Math.floor((Date.now() - start) / 1000)} s`; }, 1000);
  return () => { clearInterval(timer); box.remove(); };
}

const SVG = "http://www.w3.org/2000/svg";
function el(tag, attrs, text) {
  const node = document.createElementNS(SVG, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Bars sized against the largest: [{label, size, shown}]. */
function bars(parent, rows, color) {
  const top = Math.max(1, ...rows.map((row) => row.size));
  parent.innerHTML = rows.map((row) => `
    <div class="bar-row">
      <span class="label" title="${escape(row.label)}">${escape(row.label)}</span>
      <span class="track"><span class="fill" style="width:${Math.max(1.5, 100 * row.size / top)}%;--fill:${color}"></span></span>
      <span class="value">${escape(row.shown)}</span>
    </div>`).join("") || `<p class="muted">${escape(t("Rien pour l'instant : lancez une analyse."))}</p>`;
}

// A floating window is dragged from anywhere but its controls and lists of text.
document.addEventListener("mousedown", (event) => {
  if (event.button !== 0) return;
  if (event.target.closest("button, input, label, a, summary, .item, .row, .bar-row, .legend, .line, select, textarea")) return;
  window.__TAURI__.window.getCurrentWindow().startDragging();
});

// The pages' errors go to the program's output, where they can be read.
const report = (message) => invoke("js_error", { page: location.pathname, message: String(message) });
window.addEventListener("error", (event) => report(`${event.message} (${event.filename}:${event.lineno})`));
window.addEventListener("unhandledrejection", (event) => report(event.reason));
