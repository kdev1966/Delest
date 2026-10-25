// The menu-bar window: the disk at a glance and the way into the main window.

const ENGLISH_WORDS = {
  "Libres": "Free", "À libérer": "To free", "Supprimable": "Deletable", "Avec réseau": "With network",
  "À juger": "To review", "Les plus gros": "The heaviest", "Propositions": "Proposals",
  "ouvrir Delest ↗": "open Delest ↗", "sur": "of", "libres sur": "free of", "Fermer": "Close",
};

// The dial: 40 segments over 270°, lit from the left with the free space, then with
// what Delest can free; a red mark outside the ring at the alert threshold.
const CX = 66, CY = 74, R = 52, N = 40, FROM = -135, SWEEP = 270, GAP = 2.2;
const point = (deg, r = R) => `${(CX + r * Math.sin(rad(deg))).toFixed(2)} ${(CY - r * Math.cos(rad(deg))).toFixed(2)}`;
function rad(deg) { return deg * Math.PI / 180; }
function segments(from, to) {
  const step = SWEEP / N;
  let d = "";
  for (let i = from; i < to; i++) {
    const a = FROM + i * step + GAP / 2, b = FROM + (i + 1) * step - GAP / 2;
    d += `M${point(a)} A${R} ${R} 0 0 1 ${point(b)}`;
  }
  return d;
}

function drawDial(o) {
  const dial = $("dial");
  dial.innerHTML = "";
  const freeable = o.rebuildable + o.online;
  const free = Math.min(N, Math.round(N * o.free / Math.max(1, o.total)));
  const lit = freeable > 0 ? Math.max(1, Math.round(N * freeable / Math.max(1, o.total))) : 0;
  const path = (d, stroke) => dial.append(el("path", { d, stroke, "stroke-width": 9, fill: "none" }));
  path(segments(0, N), "var(--unlit)");
  path(segments(0, free), "var(--free)");
  path(segments(free, Math.min(N, free + lit)), "var(--accent)");
  const alert = FROM + SWEEP * Math.min(1, o.alert_bytes / Math.max(1, o.total));
  dial.append(el("path", { d: `M${point(alert, R + 6)} L${point(alert, R + 12)}`, stroke: "var(--danger)", "stroke-width": 2, fill: "none" }));
  dial.setAttribute("aria-label", `${o.free_shown} ${t("libres sur")} ${o.total_shown}`);
}

async function refresh() {
  try {
    const o = await invoke("overview");
    drawDial(o);
    $("free").textContent = o.free_shown;
    $("of").textContent = `${t("sur")} ${o.total_shown}`;
    $("rebuildable").textContent = size(o.rebuildable);
    $("online").textContent = size(o.online);
    $("review").textContent = size(o.to_review);
    $("top").innerHTML = o.projects.slice(0, 5).map((project) =>
      `<div class="line"><span>${escape(project.label)}</span><b>${escape(project.shown)}</b></div>`).join("");
  } catch (error) {
    $("top").textContent = String(error);
  }
}

$("open").addEventListener("click", () => invoke("open_main"));
$("close").addEventListener("click", () => invoke("close_window", { label: "barre" }));
$("proposals").addEventListener("click", async () => {
  await invoke("open_main");
  await emit("goto", "propositions");
});
// Fresh figures each time the window is shown, and when the main window hands over.
document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
window.addEventListener("focus", refresh);
listen("refresh", refresh);
translate(ENGLISH_WORDS).then(refresh);
