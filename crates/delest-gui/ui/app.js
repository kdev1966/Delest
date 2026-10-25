// The main window: Disk, Proposals, Quarantine, Settings.

const ENGLISH_WORDS = {
  "Disque": "Disk", "Propositions": "Proposals", "Quarantaine": "Quarantine", "Réglages": "Settings", "réponse gardée": "kept answer", "Redemander": "Ask again", "Langue": "Language", "À propos": "About", "Le projet sur GitHub": "The project on GitHub", "licence MIT": "MIT licence", "Système": "System",
  "Imposée par DELEST_LANG : retirez la variable pour choisir ici.": "Set by DELEST_LANG: remove the variable to choose here.",
  "Pour la fenêtre et pour la commande delest.": "For the window and for the delest command.",
  "Analyser": "Scan", "Par type de dossier": "By kind of folder", "Les projets qui pèsent le plus": "The heaviest projects",
  "Espace libre": "Free space", "Un point par jour, gardé 90 jours ; la bande orange est sous le seuil d'alerte.": "One point a day, kept 90 days; the orange band is below the alert threshold.",
  "Cochez ce qui part en quarantaine ; rien ne bouge avant votre confirmation.": "Check what goes into the quarantine; nothing moves before you confirm.",
  "Aperçu avant la quarantaine": "Preview before the quarantine", "Libre avant": "Free before", "Libre après suppression": "Free once deleted",
  "Mettre en quarantaine": "Move into the quarantine", "Annuler": "Cancel", "Voir l'aperçu": "See the preview",
  "Supprimer définitivement": "Delete for good", "Supprimer définitivement la sélection": "Delete the selection for good",
  "Seul ce qui se reconstruit localement peut être supprimé avant 7 jours.": "Only what rebuilds locally can be deleted before 7 days.",
  "Contrôle à chaque session des agents": "A check at each agent session", "Hook installé dans": "Hook installed in", "Réinstaller": "Reinstall", "Hook retiré de": "Hook removed from", " ; l'ancien fichier est sauvegardé.": "; the previous file is saved.", "Aucun agent pris en charge trouvé.": "No supported agent found.", "Ses réglages ne sont pas lisibles sans risque : rien n'est modifié.": "Its settings cannot be read safely: nothing is changed.", "Au début de chaque session, Delest regarde l'espace libre ; il ne dit rien sauf alerte.": "At the start of each session, Delest looks at the free space; it says nothing unless there is an alert.", "Contrôle toutes les 4 heures": "A check every 4 hours",
  "Dossiers protégés": "Protected folders", "Protéger": "Protect", "Règles en ombre": "Rules in shadow",
  "Delest ne propose jamais un dossier protégé, ni rien de ce qu'il contient.": "Delest never offers a protected folder, nor anything inside it.",
  "Rien pour l'instant : lancez une analyse.": "Nothing yet: run a scan.", "Jauge flottante": "Floating gauge",
  "Activer": "Turn on", "Activé.": "On.", "Désactiver": "Turn off", "Désactivé (par défaut).": "Off (the default).",
  "Analyse en cours : quelques dizaines de secondes pour un grand dossier.": "Scanning: a few tens of seconds for a large folder.", "Analyse…": "Scanning…", "Après nettoyage": "After cleaning",
  "Aucun dossier inconnu : tout ce qui pèse dans vos projets est reconnu par une règle ou protégé.": "No unknown folder: everything that weighs in your projects is recognised by a rule or protected.",
  "Aucun dossier protégé.": "No protected folder.", "Aucun scan encore.": "No scan yet.", "Aucune règle en ombre.": "No rule in shadow.",
  "Aucune règle ne reconnaît ces dossiers. Le modèle donne son avis ; rien n'est coché d'avance, et un inconnu reste au mieux « à juger ».": "No rule recognises these folders. The model gives its opinion; nothing is checked in advance, and an unknown is at best “to review”.",
  "Avec réseau": "With network", "Avis du modèle": "Model's opinion",
  "Besoin d'un avis plus sûr ? Delest peut poser la même question à votre agent.": "Need a surer opinion? Delest can ask your agent the same question.",
  "Ces nouvelles règles reconnaissent leurs dossiers sans rien proposer. Pour en accepter une après le rejeu du corpus : delest shadow accept <règle>.": "These new rules recognise their folders without proposing anything. To accept one after the corpus replay: delest shadow accept <rule>.",
  "Chaque dossier est vérifié de nouveau juste avant : il reste en place s'il a changé, s'il vient d'être modifié ou si un programme l'utilise. Tout reste restaurable 7 jours ; l'espace se libère à la fin de ce délai, ou tout de suite avec « Supprimer définitivement » dans la Quarantaine.": "Each folder is checked again just before: it stays in place if it changed, was just modified, or a program is using it. Everything stays restorable for 7 days; the space is freed at the end of that time, or at once with “Delete for good” in the Quarantine.",
  "Claude Code introuvable : l'agent ne peut pas être sollicité.": "Claude Code not found: the agent cannot be asked.", "Claude Code réfléchit…": "Claude Code is thinking…",
  "Claude Code, sollicité seulement sur votre clic": "Claude Code, asked only when you click", "Combien de temps pour le reconstruire ?": "How long to rebuild it?", "Demander": "Ask",
  "Demander l'avis du modèle": "Ask the model's opinion", "Demander à Claude Code": "Ask Claude Code", "Demander à Delest": "Ask Delest", "Dernier scan": "Last scan", "Dossier": "Folder",
  "Erreur": "Error",
  "Le moteur ou les modèles manquent : Delest fonctionne, mais sans avis sur les dossiers inconnus.": "The engine or the models are missing: Delest works, but gives no opinion on unknown folders.",
  "Installer l'IA locale": "Install the local AI", "Recherche de ce qui manque…": "Looking for what is missing…",
  "copie trouvée sur le disque": "copy found on the disk", "À télécharger": "To download",
  "Depuis github.com/ggml-org/llama.cpp et huggingface.co ; chaque fichier est vérifié par son empreinte avant d'être gardé. Rien d'autre n'est envoyé.": "From github.com/ggml-org/llama.cpp and huggingface.co; each file is checked against its fingerprint before it is kept. Nothing else is sent.",
  "Télécharger et installer": "Download and install", "L'installation s'est arrêtée": "The installation stopped",
  "L'IA locale est installée.": "The local AI is installed.", "Installation": "Installing", "Gemma 4 E2B peut tourner maintenant": "Gemma 4 E2B can run now", "IA": "AI",
  "Ils ne pourront plus être restaurés ; ils se reconstruiront à la prochaine compilation.": "They can no longer be restored; they will be rebuilt at the next build.", "Inconnus": "Unknown",
  "Installez d'abord la commande : cargo install --path crates/delest": "Install the command first: cargo install --path crates/delest",
  "Installé : Delest vérifie le disque au début de chaque session.": "Installed: Delest checks the disk at the start of each session.", "Jamais": "Never", "Journal des avis": "Journal of opinions",
  "La quarantaine est vide.": "The quarantine is empty.", "Laissés en place": "Left in place",
  "Le hook pointe vers un autre programme : réinstallez-le.": "The hook points to another program: reinstall it.",
  "Le journal se remplit dès que le modèle donne un avis.": "The journal fills as soon as the model gives an opinion.",
  "Le modèle local réfléchit… (le premier lancement prend quelques secondes)": "The local model is thinking… (the first start takes a few seconds)",
  "Le modèle local répond sur la machine. Si vous demandez à Claude Code, le chemin du dossier et les noms de son contenu lui sont envoyés, jamais le contenu des fichiers. Ni l'un ni l'autre ne peut rendre un dossier supprimable.": "The local model answers on this machine. If you ask Claude Code, the folder's path and the names of what it holds are sent to it, never the files' contents. Neither can make a folder deletable.",
  "Le modèle ne traite que les inconnus, et seulement vers la prudence : il ne rend jamais un dossier supprimable, ne lève aucun interdit et n'agit jamais sans vous.": "The model only handles the unknowns, and only towards caution: it never makes a folder deletable, never lifts a ban and never acts without you.",
  "Le modèle réfléchit…": "The model is thinking…", "Le reste du disque": "The rest of the disk", "Libre": "Free", "Libéré": "Freed", "Mis en quarantaine": "Moved into the quarantine",
  "Modèle": "Model", "Modèle local": "Local model", "N'est plus protégé": "No longer protected",
  "Pas encore d'avis : « Demander l'avis du modèle » ci-dessus.": "No opinion yet: “Ask the model's opinion” above.", "Pas encore disponible sur ce système.": "Not available on this system yet.",
  "Pas installé.": "Not installed.", "Proche de": "Close to", "Protégé": "Protected", "Quand": "When", "Que se passe-t-il si je le supprime ?": "What happens if I delete it?",
  "Reconnus": "Recognised", "Remis en place": "Put back", "Restaurer": "Restore", "Retirer": "Remove", "Retour aux propositions": "Back to the proposals",
  "Rien à proposer : lancez une analyse depuis la page Disque.": "Nothing to propose: run a scan from the Disk page.", "Supprimable": "Deletable", "Sur demande": "On request",
  "Sélection": "Selection", "Tout": "All", "Voir les": "See the", "Votre décision": "Your decision", "Votre question": "Your question", "absent": "absent", "alerte sous": "alert below",
  "batterie": "battery", "dans": "in", "dossier": "folder", "dossiers": "folders", "dossier sans avis": "folder without an opinion", "dossiers sans avis": "folders without an opinion",
  "encore": "yet", "environ 20 s par dossier": "about 20 s per folder", "jour": "day", "jours": "days", "le disque se remplit": "the disk is filling up",
  "les premiers sont supprimés dans": "the first ones are deleted in", "libres": "free", "libres sur": "free of", "moteur absent": "no engine",
  "moteur llama.cpp installé": "llama.cpp engine installed", "mémoire libre": "free memory",
  "pas assez de mémoire libre ou batterie faible : aucun modèle maintenant": "not enough free memory or low battery: no model now", "peuvent être libérés": "can be freed", "projet": "project",
  "projets": "projects", "rien à signaler": "nothing to report", "seul le modèle de repli peut tourner maintenant": "only the fallback model can run now", "thermostat Vario": "Vario's thermostat",
  "À juger": "To review", "À quoi sert ce dossier ?": "What is this folder for?", "à garder": "to keep", "à juger": "to review", "élément": "item", "éléments": "items",
  "Dossier (par défaut : le dernier)": "Folder (default: the last one)", "Dossier à analyser": "Folder to scan", "Dossier à protéger": "Folder to protect",
  "Fermer": "Close", "Historique de l'espace libre": "Free space history", "Posez une question sur ce dossier…": "Ask a question about this folder…",
  "Sections": "Sections", "Espace": "Space", "Quitter Delest": "Quit Delest", "Types de dossier": "Kinds of folder", "Votre agent": "Your agent", "Vue": "View",
};

let proposals = [];
let unknownItems = [];
let showUnknown = false;
const checked = new Set();
let filter = null;

function show(page) {
  for (const tab of document.querySelectorAll(".tab")) {
    tab.toggleAttribute("aria-current", tab.dataset.page === page);
    if (tab.dataset.page === page) tab.setAttribute("aria-current", "page");
  }
  for (const section of document.querySelectorAll("main > section")) section.hidden = section.id !== `page-${page}`;
  ({ disque: loadDisk, propositions: loadProposals, quarantaine: loadQuarantine, reglages: loadSettings, demander: () => {} })[page]();
}

function message(id, text, error) {
  const node = $(id);
  node.hidden = !text;
  node.textContent = text || "";
  node.classList.toggle("error", Boolean(error));
}

// ------------------------------------------------------------------ Disk

/** The disk as a row of 50 lit segments: the rest of the disk, then what can be
 * freed by kind, then the free space. A part too small for a segment still gets one. */
const SEGMENTS = 50;
function diskSegments(o) {
  const parts = [[o.rebuildable, "var(--accent)"], [o.online, "var(--online)"], [o.to_review, "var(--review)"], [o.free, "var(--free)"]];
  const counts = parts.map(([value]) => value > 0 ? Math.max(1, Math.round(SEGMENTS * value / Math.max(1, o.total))) : 0);
  const used = Math.max(0, SEGMENTS - counts.reduce((sum, count) => sum + count, 0));
  const colors = Array(used).fill("var(--used)");
  parts.forEach(([, color], index) => colors.push(...Array(counts[index]).fill(color)));
  return colors.slice(0, SEGMENTS).map((color) => `<span style="background:${color}"></span>`).join("");
}

function drawRing(o) {
  $("space-free").textContent = o.free_shown;
  $("space-of").textContent = `${t("libres sur")} ${o.total_shown}`;
  const bar = $("disk-bar");
  bar.innerHTML = diskSegments(o);
  bar.setAttribute("aria-label", `${o.free_shown} ${t("libres sur")} ${o.total_shown}`);
  const row = (color, label, value) => `<span class="swatch" style="background:${color}"></span><span>${escape(label)}</span><b class="mono" style="font-weight:500">${escape(value)}</b>`;
  $("legend").innerHTML = [
    row("var(--accent)", t("Supprimable"), size(o.rebuildable)),
    row("var(--online)", t("Avec réseau"), size(o.online)),
    row("var(--review)", t("À juger"), size(o.to_review)),
    row("var(--free)", t("Libre"), o.free_shown),
    row("var(--used)", t("Le reste du disque"), size(Math.max(0, o.total - o.free - o.rebuildable - o.online - o.to_review))),
  ].join("") + `<div class="after">${escape(t("Après nettoyage"))}${colon()}<b>${escape(o.after_shown)} ${escape(t("libres"))}</b></div>`;
}

function drawHistory(o) {
  const svg = $("history");
  svg.innerHTML = "";
  const [left, right, top, bottom] = [44, 450, 14, 140];
  const days = o.history;
  const highest = Math.max(o.alert_bytes * 1.6, ...days.map((day) => day.available)) * 1.1;
  const y = (bytes) => bottom - (bottom - top) * bytes / highest;
  const first = days.length ? days[0].at : 0, last = days.length ? days[days.length - 1].at : 1;
  const x = (at) => days.length < 2 ? right : left + (right - left) * (at - first) / Math.max(1, last - first);
  svg.append(el("rect", { x: left, y: y(o.alert_bytes), width: right - left, height: bottom - y(o.alert_bytes), fill: "#3a2f17" }));
  svg.append(el("line", { x1: left, y1: y(o.alert_bytes), x2: right, y2: y(o.alert_bytes), stroke: "var(--review)", "stroke-dasharray": "4 4" }));
  svg.append(el("text", { x: right, y: y(o.alert_bytes) + 14, "text-anchor": "end", "font-size": 11, fill: "var(--review)" }, `${t("alerte sous")} ${size(o.alert_bytes)}`));
  svg.append(el("line", { x1: left, y1: bottom, x2: right, y2: bottom, stroke: "var(--edge)" }));
  svg.append(el("text", { x: left - 6, y: bottom + 4, "text-anchor": "end", "font-size": 11, fill: "var(--muted)" }, "0"));
  if (days.length) {
    svg.append(el("polyline", { points: days.map((day) => `${x(day.at)},${y(day.available)}`).join(" "), fill: "none", stroke: "var(--accent)", "stroke-width": 2.5 }));
    const end = days[days.length - 1];
    svg.append(el("circle", { cx: x(end.at), cy: y(end.available), r: 5, fill: "var(--bg)", stroke: "var(--accent)", "stroke-width": 2.5 }));
    svg.append(el("text", { x: x(end.at) - 10, y: y(end.available) - 10, "text-anchor": "end", "font-size": 12, "font-weight": 600, fill: "var(--text)" }, size(end.available)));
    const date = (at) => new Date(at * 1000).toLocaleDateString(ENGLISH ? "en" : "fr", { day: "numeric", month: "short" });
    svg.append(el("text", { x: left, y: bottom + 18, "font-size": 11, fill: "var(--muted)" }, date(first)));
    svg.append(el("text", { x: right, y: bottom + 18, "text-anchor": "end", "font-size": 11, fill: "var(--muted)" }, date(last)));
  }
}

function drawDisk(o) {
  $("free-line").textContent = `${o.free_shown} ${t("libres sur")} ${o.total_shown}`;
  const level = $("level");
  level.className = `chip ${o.level}`;
  level.textContent = { fine: t("rien à signaler"), notice: `${o.freeable_shown} ${t("peuvent être libérés")}`, alert: t("le disque se remplit") }[o.level];
  drawRing(o);
  bars($("kinds"), o.categories, "var(--accent)");
  bars($("projects"), o.projects, "var(--accent)");
  $("see-all").textContent = `${t("Voir les")} ${o.project_count} ${t("projets")}`;
  $("see-all").hidden = !o.project_count;
  drawHistory(o);
  $("scan-note").textContent = o.scanned ? `${t("Dernier scan")}${colon()}${o.roots.join(", ")}, ${o.scanned}` : t("Aucun scan encore.");
}

async function loadDisk() {
  try { drawDisk(await invoke("overview")); message("disk-message", ""); }
  catch (error) { message("disk-message", error, true); }
}

$("rescan").addEventListener("click", async () => {
  const button = $("rescan");
  button.disabled = true;
  button.textContent = t("Analyse…");
  message("disk-message", t("Analyse en cours : quelques dizaines de secondes pour un grand dossier."));
  try { drawDisk(await invoke("rescan", { folder: $("folder").value || null })); message("disk-message", ""); }
  catch (error) { message("disk-message", error, true); }
  button.disabled = false;
  button.textContent = t("Analyser");
});
$("see-all").addEventListener("click", () => show("propositions"));

// ------------------------------------------------------------------ Proposals

function visible(item) { return !filter || item.kind === filter; }

function selection() {
  const items = proposals.flatMap((project) => project.items).concat(unknownItems).filter((item) => checked.has(item.path));
  return { count: items.length, size: items.reduce((sum, item) => sum + item.size, 0), paths: items.map((item) => item.path) };
}

function drawSelection() {
  const { count, size: bytes } = selection();
  $("selection-line").innerHTML = `${escape(t("Sélection"))}${colon()}<b>${escape(size(bytes))}</b> ${escape(t("dans"))} ${count} ${escape(t(count === 1 ? "dossier" : "dossiers"))}`;
  $("see-preview").disabled = count === 0;
}

function drawProposals() {
  const kinds = [...new Set(proposals.flatMap((project) => project.items.map((item) => item.kind)))];
  $("filters").innerHTML = [`<button class="filter" aria-pressed="${!filter}" data-kind="">${escape(t("Tout"))}</button>`]
    .concat(kinds.map((kind) => `<button class="filter" aria-pressed="${filter === kind}" data-kind="${escape(kind)}">${escape(kind)}</button>`)).join("");
  const projects = proposals.filter((project) => project.items.some(visible));
  $("project-list").innerHTML = projects.map((project, index) => {
    const items = project.items.filter(visible);
    const all = items.every((item) => checked.has(item.path));
    return `<details class="project" ${index < 3 ? "open" : ""}>
      <summary>
        <input type="checkbox" data-project="${escape(project.root)}" ${all ? "checked" : ""} aria-label="${escape(project.name)}">
        <span class="name">${escape(project.name)}</span>
        <span class="root">${escape(project.root)}</span>
        <span class="total">${escape(project.size_shown)}</span>
      </summary>
      ${items.map((item) => `<label class="item">
        <input type="checkbox" data-path="${escape(item.path)}" ${checked.has(item.path) ? "checked" : ""}>
        <span class="what"><b>${escape(item.shown)}</b> · ${escape(item.kind)}${item.note ? ` <span class="note">· ${escape(item.note)}</span>` : ""}</span>
        <span class="value">${escape(item.size_shown)}</span>
        <span class="trace">${escape(item.trace)} · <button type="button" class="ask-link" data-ask="${escape(item.path)}" data-shown="${escape(project.name + "/" + item.shown)}">${escape(t("Demander"))}</button></span>
      </label>`).join("")}
    </details>`;
  }).join("") || `<p class="muted">${escape(t("Rien à proposer : lancez une analyse depuis la page Disque."))}</p>`;
  drawSelection();
}

function drawViews() {
  const unknownSize = unknownItems.reduce((sum, item) => sum + item.size, 0);
  $("view-known").textContent = `${t("Reconnus")} · ${proposals.length} ${t(proposals.length === 1 ? "projet" : "projets")}`;
  $("view-unknown").textContent = `${t("Inconnus")} · ${unknownItems.length} · ${size(unknownSize)}`;
  $("view-known").setAttribute("aria-pressed", String(!showUnknown));
  $("view-unknown").setAttribute("aria-pressed", String(showUnknown));
  $("known-view").hidden = showUnknown;
  $("unknown-view").hidden = !showUnknown;
}

function drawUnknowns() {
  const waiting = unknownItems.filter((item) => !item.opinion).length;
  $("advise").hidden = waiting === 0;
  $("advise-note").textContent = waiting ? `${waiting} ${t(waiting === 1 ? "dossier sans avis" : "dossiers sans avis")}${semi()}${t("environ 20 s par dossier")}` : "";
  $("unknown-list").innerHTML = unknownItems.map((item) => {
    const o = item.opinion;
    const review = o && o.advice === "review";
    const advice = o ? `<span class="chip ${review ? "review" : "keep"}">${escape(t(review ? "à juger" : "à garder"))}</span>` : "";
    return `<div class="unknown">
      <input type="checkbox" data-path="${escape(item.path)}" ${review ? "" : "disabled"} ${checked.has(item.path) ? "checked" : ""} aria-label="${escape(item.shown)}">
      <span><b>${escape(item.shown)}</b> <span class="muted" style="font-size:10px">· ${escape(item.trace)}</span></span>
      <b class="mono" style="text-align:right;font-weight:500">${escape(item.size_shown)}</b>
      <div class="opinion">
        ${o ? `<div class="head"><span class="who">${escape(t("Avis du modèle"))} · ${escape(o.model)}</span>${advice}<span class="muted" style="margin-left:auto;font-size:10px">${o.seconds.toFixed(0)} s</span></div>
        <span style="font-size:11px"><b>${escape(o.nature)}</b> — ${escape(o.reason)}</span>
        <span class="why">${escape(t("Proche de"))}${colon()}${escape(o.neighbours.join(semi()))}</span>`
        : `<span class="why">${escape(t("Pas encore d'avis : « Demander l'avis du modèle » ci-dessus."))}</span>`}
        <button type="button" class="ask-link" style="align-self:flex-start" data-ask="${escape(item.path)}" data-shown="${escape(item.shown)}">${escape(t("Demander à Delest"))}</button>
      </div>
    </div>`;
  }).join("") || `<p class="muted">${escape(t("Aucun dossier inconnu : tout ce qui pèse dans vos projets est reconnu par une règle ou protégé."))}</p>`;
  drawSelection();
}

async function loadProposals() {
  $("confirm").hidden = true;
  try {
    [proposals, unknownItems] = await Promise.all([invoke("proposals"), invoke("unknowns")]);
    checked.clear();
    for (const item of proposals.flatMap((project) => project.items)) if (item.suggested) checked.add(item.path);
    drawViews();
    drawProposals();
    drawUnknowns();
  } catch (error) { message("clean-message", error, true); }
}

$("view-known").addEventListener("click", () => { showUnknown = false; drawViews(); });
$("view-unknown").addEventListener("click", () => { showUnknown = true; drawViews(); });
$("unknown-list").addEventListener("change", (event) => {
  const box = event.target;
  if (!box.dataset.path) return;
  box.checked ? checked.add(box.dataset.path) : checked.delete(box.dataset.path);
  drawSelection();
});
$("advise").addEventListener("click", async () => {
  const button = $("advise");
  button.disabled = true;
  $("advise-note").textContent = "";
  const stop = thinking($("advise-note"), t("Le modèle réfléchit…"));
  try { await invoke("advise_unknowns"); stop(); unknownItems = await invoke("unknowns"); drawViews(); drawUnknowns(); }
  catch (error) { stop(); message("clean-message", error, true); }
  button.disabled = false;
});

// ------------------------------------------------------------------ Ask Delest

let asking = null;
let lastQuestion = "";

function bubble(kind, text, by) {
  const node = document.createElement("div");
  node.className = `bubble ${kind}`;
  node.innerHTML = (by ? `<span class="by">${escape(by)}</span>` : "") + escape(text);
  $("conversation").append(node);
  return node;
}

function openAsk(path, shown) {
  asking = path;
  $("ask-folder").textContent = shown;
  $("conversation").innerHTML = "";
  $("agent-offer").hidden = true;
  show("demander");
  $("question").focus();
}

/** Asks the local model or the agent; an answer kept from before comes back at once,
 *  with a button to ask again, which answers in the same bubble. */
async function sendQuestion(question, toAgent, fresh = false, into = null) {
  if (!question.trim() || !asking) return;
  lastQuestion = question;
  if (!toAgent && !into) bubble("me", question);
  const waiting = into || bubble("it", "", toAgent ? "Claude Code" : t("Modèle local"));
  if (into) into.innerHTML = `<span class="by">${escape(toAgent ? "Claude Code" : t("Modèle local"))}</span>`;
  const stop = thinking(waiting, t(toAgent ? "Claude Code réfléchit…" : "Le modèle local réfléchit… (le premier lancement prend quelques secondes)"));
  $("ask-send").disabled = true;
  try {
    const answer = await invoke(toAgent ? "ask_agent" : "ask", { path: asking, question, fresh });
    stop();
    const when = answer.kept ? `${t("réponse gardée")}, ${answer.kept}` : `${answer.seconds.toFixed(0)} s`;
    waiting.innerHTML = `<span class="by">${escape(answer.by)} · ${escape(when)}</span>${escape(answer.text)}`;
    if (answer.kept) {
      const again = document.createElement("button");
      again.className = "ask-link again";
      again.textContent = t("Redemander");
      again.addEventListener("click", () => sendQuestion(question, toAgent, true, waiting));
      waiting.append(again);
    }
    $("agent-offer").hidden = toAgent;
  } catch (error) {
    stop();
    waiting.innerHTML = `<span class="by">${escape(t("Erreur"))}</span>${escape(error)}`;
  }
  $("ask-send").disabled = false;
}

document.addEventListener("click", (event) => {
  const link = event.target.closest("[data-ask]");
  if (!link) return;
  event.preventDefault();
  event.stopPropagation();
  openAsk(link.dataset.ask, link.dataset.shown);
}, true);
$("ask-back").addEventListener("click", () => show("propositions"));
$("suggestions").addEventListener("click", (event) => {
  const button = event.target.closest(".suggest");
  if (button) sendQuestion(button.textContent, false);
});
$("ask-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const question = $("question").value;
  $("question").value = "";
  sendQuestion(question, false);
});
$("ask-agent").addEventListener("click", () => sendQuestion(lastQuestion, true));

$("filters").addEventListener("click", (event) => {
  const button = event.target.closest(".filter");
  if (!button) return;
  filter = button.dataset.kind || null;
  drawProposals();
});
$("project-list").addEventListener("click", (event) => {
  if (event.target.matches("summary input[type=checkbox]")) event.stopPropagation();
});
$("project-list").addEventListener("change", (event) => {
  const box = event.target;
  if (box.dataset.path) {
    box.checked ? checked.add(box.dataset.path) : checked.delete(box.dataset.path);
  } else if (box.dataset.project) {
    const project = proposals.find((candidate) => candidate.root === box.dataset.project);
    for (const item of project.items.filter(visible)) box.checked ? checked.add(item.path) : checked.delete(item.path);
  }
  drawProposals();
});

$("see-preview").addEventListener("click", async () => {
  const { paths } = selection();
  try {
    const preview = await invoke("preview", { paths });
    $("confirm-line").innerHTML = `${escape(t("Mettre en quarantaine"))} <b>${preview.count} ${escape(t(preview.count === 1 ? "dossier" : "dossiers"))}</b>, <b>${escape(preview.size_shown)}</b>.`;
    $("free-before").textContent = preview.free_shown;
    $("free-after").textContent = preview.after_shown;
    $("bar-before").style.width = `${100 * preview.free / preview.after}%`;
    $("bar-after").style.width = "100%";
    $("confirm").hidden = false;
    $("confirm").scrollIntoView({ behavior: "smooth", block: "center" });
    message("clean-message", "");
  } catch (error) { message("clean-message", error, true); }
});
$("cancel-quarantine").addEventListener("click", () => { $("confirm").hidden = true; });
$("do-quarantine").addEventListener("click", async () => {
  const button = $("do-quarantine");
  button.disabled = true;
  try {
    const moved = await invoke("quarantine_items", { paths: selection().paths });
    $("confirm").hidden = true;
    const skipped = moved.skipped.length ? ` ${t("Laissés en place")}${colon()}${moved.skipped.join(semi())}.` : "";
    await loadProposals();
    message("clean-message", `${t("Mis en quarantaine")}${colon()}${moved.count} ${t(moved.count === 1 ? "dossier" : "dossiers")} (${moved.size_shown}).${skipped}`);
  } catch (error) { message("clean-message", error, true); }
  button.disabled = false;
});

// ------------------------------------------------------------------ Quarantine

let entries = [];
const toPurge = new Set();

function drawQuarantine() {
  const total = entries.reduce((sum, entry) => sum + entry.size, 0);
  const next = entries.length ? Math.min(...entries.map((entry) => entry.days_left)) : 0;
  $("quarantine-line").textContent = entries.length
    ? `${entries.length} ${t(entries.length === 1 ? "élément" : "éléments")}, ${size(total)}${semi()}${t("les premiers sont supprimés dans")} ${next} ${t(next === 1 ? "jour" : "jours")}`
    : t("La quarantaine est vide.");
  $("quarantine-list").innerHTML = entries.map((entry) => `<div class="row">
      <input type="checkbox" data-id="${escape(entry.id)}" ${entry.may_purge ? "" : "disabled"} ${toPurge.has(entry.id) ? "checked" : ""} aria-label="${escape(entry.path)}">
      <span><b>${escape(entry.path)}</b><br><span class="muted" style="font-size:10px">${escape(entry.kind)}</span></span>
      <span class="mono" style="text-align:right">${escape(entry.size_shown)}</span>
      <span class="muted" style="font-size:10px">${escape(t("encore"))} ${entry.days_left} ${escape(t(entry.days_left === 1 ? "jour" : "jours"))}</span>
      <button data-restore="${escape(entry.id)}">${escape(t("Restaurer"))}</button>
    </div>`).join("");
  $("quarantine-list").hidden = !entries.length;
  $("ask-purge").disabled = toPurge.size === 0;
}

async function loadQuarantine() {
  $("purge-confirm").hidden = true;
  try { entries = await invoke("quarantine_list"); toPurge.clear(); drawQuarantine(); }
  catch (error) { message("quarantine-message", error, true); }
}

$("quarantine-list").addEventListener("change", (event) => {
  const id = event.target.dataset.id;
  if (!id) return;
  event.target.checked ? toPurge.add(id) : toPurge.delete(id);
  $("ask-purge").disabled = toPurge.size === 0;
});
$("quarantine-list").addEventListener("click", async (event) => {
  const id = event.target.dataset && event.target.dataset.restore;
  if (!id) return;
  try {
    const path = await invoke("restore", { id });
    await loadQuarantine();
    message("quarantine-message", `${t("Remis en place")}${colon()}${path}`);
  } catch (error) { message("quarantine-message", error, true); }
});
$("ask-purge").addEventListener("click", () => {
  const chosen = entries.filter((entry) => toPurge.has(entry.id));
  const bytes = chosen.reduce((sum, entry) => sum + entry.size, 0);
  $("purge-line").innerHTML = `${escape(t("Supprimer définitivement"))} <b>${chosen.length} ${escape(t(chosen.length === 1 ? "élément" : "éléments"))}, ${escape(size(bytes))}</b>${ENGLISH ? "?" : " ?"} ${escape(t("Ils ne pourront plus être restaurés ; ils se reconstruiront à la prochaine compilation."))}`;
  $("purge-confirm").hidden = false;
});
$("cancel-purge").addEventListener("click", () => { $("purge-confirm").hidden = true; });
$("do-purge").addEventListener("click", async () => {
  try {
    const freed = await invoke("purge", { ids: [...toPurge] });
    await loadQuarantine();
    message("quarantine-message", `${t("Libéré")}${colon()}${freed}`);
  } catch (error) { message("quarantine-message", error, true); }
});

// ------------------------------------------------------------------ Settings

let current = null;

function drawSettings(s) {
  current = s;
  const states = {
    installed: t("Installé : Delest vérifie le disque au début de chaque session."),
    absent: t("Pas installé."),
    outdated: t("Le hook pointe vers un autre programme : réinstallez-le."),
    unreadable: t("Ses réglages ne sont pas lisibles sans risque : rien n'est modifié."),
    "no-command": t("Installez d'abord la commande : cargo install --path crates/delest"),
  };
  $("hooks").innerHTML = s.hooks.map((hook) => `<div class="setting"><div><b>${escape(hook.name)}</b>
      <p class="muted" style="margin:2px 0 0">${escape(states[hook.state])}</p></div>
      <button data-hook="${escape(hook.agent)}" data-on="${hook.state !== "installed"}" ${hook.state === "no-command" || hook.state === "unreadable" ? "disabled" : ""}>${escape(t({ installed: "Retirer", outdated: "Réinstaller" }[hook.state] || "Installer"))}</button></div>`).join("")
    || `<p class="muted">${escape(t("Aucun agent pris en charge trouvé."))}</p>`;
  $("timer-line").textContent = !s.timer_supported ? t("Pas encore disponible sur ce système.")
    : s.timer ? t("Activé.") : t("Désactivé (par défaut).");
  $("timer-toggle").textContent = s.timer ? t("Désactiver") : t("Activer");
  $("timer-toggle").disabled = !s.timer_supported || !s.command;
  $("protected").innerHTML = s.protected.map((path) => `<div class="setting"><span class="mono">${escape(path)}</span><button data-unprotect="${escape(path)}">${escape(t("Retirer"))}</button></div>`).join("")
    || `<p class="muted">${escape(t("Aucun dossier protégé."))}</p>`;
  $("shadow").innerHTML = s.shadow.map((rule) => `<div class="bar-row" style="grid-template-columns:minmax(0,160px) minmax(0,1fr) 80px"><span class="mono">${escape(rule.rule)}</span><span class="muted">${rule.count} ${escape(t(rule.count === 1 ? "dossier" : "dossiers"))}</span><span class="value">${escape(rule.size_shown)}</span></div>`).join("")
    || `<p class="muted">${escape(t("Aucune règle en ombre."))}</p>`;
}

const LANGUAGES = { en: "English", fr: "Français" };

function drawLanguage(l) {
  for (const button of $("language").querySelectorAll("button")) {
    button.setAttribute("aria-pressed", String(button.dataset.lang === l.choice));
    button.disabled = l.forced;
  }
  $("language").querySelector('[data-lang="system"]').textContent = `${t("Système")} (${LANGUAGES[l.system]})`;
  $("language-line").textContent = l.forced ? t("Imposée par DELEST_LANG : retirez la variable pour choisir ici.")
    : t("Pour la fenêtre et pour la commande delest.");
}

// The local AI, installed on a click: what will be fetched, a confirmation, a bar.
let setupTimer = null;

async function openInstall() {
  const panel = $("ai-install-panel");
  panel.hidden = false;
  panel.classList.remove("error");
  panel.textContent = t("Recherche de ce qui manque…");
  try {
    const plan = await invoke("ai_install_plan");
    const note = plan.engine_note ? `<p class="muted">${escape(plan.engine_note)}</p>` : "";
    if (!plan.lines.length) {
      if (!note) { panel.hidden = true; return loadSettings(); }
      panel.innerHTML = note;
      return;
    }
    const lines = plan.lines.map((line) => `<div class="bar-row" style="grid-template-columns:minmax(0,1fr) auto"><span>${escape(line.label)}</span>
      <span class="value">${escape(line.from_disk ? t("copie trouvée sur le disque") : line.download_shown)}</span></div>`).join("");
    panel.innerHTML = `${note}<b>${escape(t("À télécharger"))}${colon()}${escape(plan.total_shown)}</b>${lines}
      <p class="muted" style="font-size:10px">${escape(t("Depuis github.com/ggml-org/llama.cpp et huggingface.co ; chaque fichier est vérifié par son empreinte avant d'être gardé. Rien d'autre n'est envoyé."))}</p>
      <div class="inline"><button id="ai-install-go" class="primary">${escape(t("Télécharger et installer"))}</button><button id="ai-install-cancel">${escape(t("Annuler"))}</button></div>`;
    $("ai-install-go").addEventListener("click", startInstall);
    $("ai-install-cancel").addEventListener("click", () => { panel.hidden = true; });
  } catch (error) {
    panel.classList.add("error");
    panel.textContent = String(error);
  }
}

async function startInstall() {
  try { await invoke("install_ai"); } catch (error) { return showSetup({ error: String(error) }); }
  $("ai-install-open").disabled = true;
  clearInterval(setupTimer);
  setupTimer = setInterval(async () => showSetup(await invoke("ai_install_status")), 700);
}

function showSetup(s) {
  const panel = $("ai-install-panel");
  if (s.error) {
    clearInterval(setupTimer);
    $("ai-install-open").disabled = false;
    panel.classList.add("error");
    panel.textContent = `${t("L'installation s'est arrêtée")}${colon()}${s.error}`;
    return;
  }
  const share = s.total ? Math.round(100 * s.done / s.total) : 100;
  panel.innerHTML = `<b>${escape(s.finished ? t("L'IA locale est installée.") : `${t("Installation")}${colon()}${s.step}`)}</b>
    <div class="bar-row" style="grid-template-columns:minmax(0,1fr) auto"><span class="track"><span class="fill" style="width:${share}%"></span></span>
    <span class="value">${escape(s.done_shown)} / ${escape(s.total_shown)}</span></div>`;
  if (s.finished) {
    clearInterval(setupTimer);
    $("ai-install-open").disabled = false;
    invoke("ai_settings").then(drawAi);
  }
}

function drawAi(a) {
  const reasoning = { main: t("Gemma 4 E2B peut tourner maintenant"), fallback: t("seul le modèle de repli peut tourner maintenant"), none: t("pas assez de mémoire libre ou batterie faible : aucun modèle maintenant") }[a.reasoning];
  $("ai").innerHTML = a.models.map((model) => `<div class="bar-row" style="grid-template-columns:minmax(0,150px) minmax(0,1fr) 70px">
      <span class="mono">${escape(model.name)}</span><span class="muted">${escape(model.role)}</span>
      <span class="value">${model.installed ? escape(model.size_shown) : escape(t("absent"))}</span></div>`).join("")
    + `<p class="muted" style="font-size:10px">${escape(a.store)} · ${escape(a.engine ? t("moteur llama.cpp installé") : t("moteur absent"))} · ${escape(t("mémoire libre"))} ${escape(a.free_memory)}${a.battery !== null ? ` · ${escape(t("batterie"))} ${a.battery} %` : ""} · ${escape(reasoning)}</p>`;
  $("agent-line").textContent = !a.agent_available ? t("Claude Code introuvable : l'agent ne peut pas être sollicité.")
    : `${t("Claude Code, sollicité seulement sur votre clic")}${a.thermostat ? `${semi()}${t("thermostat Vario")}${colon()}${a.thermostat}` : ""}.`;
  $("agent-on").setAttribute("aria-pressed", String(!a.agent_never));
  $("ai-install").hidden = !a.missing;
  $("agent-off").setAttribute("aria-pressed", String(a.agent_never));
}

function drawJournal(rows) {
  const head = `<div class="journal head"><span>${escape(t("Quand"))}</span><span>${escape(t("Dossier"))}</span><span>${escape(t("Avis du modèle"))}</span><span>${escape(t("Modèle"))}</span><span>${escape(t("Votre décision"))}</span></div>`;
  $("journal").innerHTML = rows.length ? head + rows.map((row) => `<div class="journal">
      <span class="muted">${escape(row.when)}</span>
      <span><b>${escape(row.path)}</b> · ${escape(row.size_shown)}</span>
      <span>${escape(row.nature)} · <span class="chip ${row.advice === "review" ? "review" : "keep"}">${escape(t(row.advice === "review" ? "à juger" : "à garder"))}</span></span>
      <span class="muted">${escape(row.model)}</span>
      <span class="muted">${escape(row.decision)}</span></div>`).join("")
    : `<p class="muted" style="padding:8px 10px">${escape(t("Le journal se remplit dès que le modèle donne un avis."))}</p>`;
}

async function loadSettings() {
  // Each part is drawn as soon as it is ready: asking Vario takes a second or more.
  const fail = (error) => message("settings-message", error, true);
  invoke("language").then(drawLanguage).catch(fail);
  invoke("about").then((a) => { $("about-line").textContent = `Delest ${a.version} · © 2026 ${a.author} · ${t("licence MIT")}`; }).catch(fail);
  invoke("settings").then(drawSettings).catch(fail);
  invoke("ai_settings").then(drawAi).catch(fail);
  invoke("journal").then(drawJournal).catch(fail);
}

$("project").addEventListener("click", () => invoke("open_project").catch((error) => message("settings-message", error, true)));
$("language").addEventListener("click", (event) => {
  const button = event.target.closest("button[data-lang]");
  if (button && button.getAttribute("aria-pressed") !== "true") {
    invoke("set_language", { choice: button.dataset.lang }).catch((error) => message("settings-message", error, true));
  }
});
$("ai-install-open").addEventListener("click", openInstall);
$("agent-on").addEventListener("click", () => change("set_agent", { never: false }, ""));
$("agent-off").addEventListener("click", () => change("set_agent", { never: true }, ""));

async function change(command, args, done) {
  try { await invoke(command, args); await loadSettings(); message("settings-message", done); }
  catch (error) { message("settings-message", error, true); }
}

$("hooks").addEventListener("click", (event) => {
  const button = event.target.closest("button[data-hook]");
  if (!button) return;
  const on = button.dataset.on === "true";
  const name = button.closest(".setting").querySelector("b").textContent;
  change("set_hook", { agent: button.dataset.hook, on },
    `${on ? t("Hook installé dans") : t("Hook retiré de")} ${name}${on ? t(" ; l'ancien fichier est sauvegardé.") : "."}`);
});
$("timer-toggle").addEventListener("click", () => change("set_timer", { on: !current.timer }, ""));
$("protect-add").addEventListener("click", () => {
  const path = $("protect-path").value.trim();
  if (path) change("set_protected", { path, on: true }, `${t("Protégé")}${colon()}${path}`).then(() => { $("protect-path").value = ""; });
});
$("protected").addEventListener("click", (event) => {
  const path = event.target.dataset && event.target.dataset.unprotect;
  if (path) change("set_protected", { path, on: false }, `${t("N'est plus protégé")}${colon()}${path}`);
});

// ------------------------------------------------------------------ Start

for (const tab of document.querySelectorAll(".tab")) tab.addEventListener("click", () => show(tab.dataset.page));
$("close").addEventListener("click", () => invoke("close_window", { label: "main" }));
$("gauge").addEventListener("click", () => invoke("open_gauge"));
listen("goto", (event) => show(event.payload));
translate(ENGLISH_WORDS).then(async () => {
  const first = await invoke("first_page");
  if (first === "inconnus") showUnknown = true;
  show(first === "inconnus" ? "propositions" : first);
  // The note on the last scan, whatever the first page.
  invoke("overview").then((o) => {
    $("scan-note").textContent = o.scanned ? `${t("Dernier scan")}${colon()}${o.roots.join(", ")}, ${o.scanned}` : t("Aucun scan encore.");
  }).catch(() => {});
});
