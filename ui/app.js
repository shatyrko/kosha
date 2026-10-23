// Kosha's window. Talks to the Rust side through Tauri's invoke; no framework, no build step.
"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const q = $("q");
const results = $("results");
const articles = $("articles");
const wordTitle = $("word");

const state = {
  ready: false,
  dicts: 0,
  hits: [],
  sel: -1,
  shown: null,      // word whose articles are on screen
  history: [],
  pos: -1,
  pending: null,    // word to show once the dictionaries are open
  token: 0,         // newest request; older answers are dropped
};

// ── small helpers ────────────────────────────────────────────
function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children) if (c != null) e.append(c);
  return e;
}

function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}

// a calm colour per dictionary, from its id
function dictColor(id) {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return `hsl(${h} 55% 55%)`;
}

function store(key, value) {
  try { localStorage.setItem(key, value); } catch { /* private mode: only for this run */ }
}
function load(key, fallback) {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}

// ── reading preferences ──────────────────────────────────────
let size = Number(load("kosha.size", 17)) || 17;
function applySize() {
  size = Math.min(28, Math.max(12, size));
  document.documentElement.style.setProperty("--article-size", `${size}px`);
  store("kosha.size", size);
}
applySize();

const secondary = $("show-secondary");
secondary.checked = load("kosha.secondary", "1") === "1";
function applySecondary() {
  document.body.classList.toggle("hide-secondary", !secondary.checked);
  store("kosha.secondary", secondary.checked ? "1" : "0");
}
secondary.addEventListener("change", applySecondary);
applySecondary();
$("smaller").onclick = () => { size -= 1; applySize(); };
$("larger").onclick = () => { size += 1; applySize(); };

// ── search ───────────────────────────────────────────────────
let previewTimer = 0;

async function runSearch() {
  const text = q.value;
  $("clear").hidden = text === "";
  if (!text.trim()) {
    state.hits = [];
    state.sel = -1;
    renderResults();
    if (!state.shown) showWelcome();
    return;
  }
  if (!state.ready) {
    renderResults();
    return;
  }
  const token = ++state.token;
  const hits = await invoke("search", { query: text, limit: 80 });
  if (token !== state.token) return;
  state.hits = hits;
  state.sel = hits.length ? 0 : -1;
  renderResults();
  clearTimeout(previewTimer);
  if (hits.length) {
    // the top word's articles while typing, as a preview (history only on Enter / click)
    previewTimer = setTimeout(() => show(hits[0].word, false), 90);
  } else {
    showNotFound(text, []);
  }
}

function renderResults() {
  results.replaceChildren();
  if (!state.ready && q.value.trim()) {
    results.append(el("li", { class: "note", text: "Opening the dictionaries…" }));
    return;
  }
  let fuzzyHeader = false;
  state.hits.forEach((h, i) => {
    if (h.fuzzy && !fuzzyHeader) {
      fuzzyHeader = true;
      results.append(el("li", { class: "group", text: i === 0 ? "Did you mean" : "Similar" }));
    }
    const li = el(
      "li",
      {
        role: "option",
        class: i === state.sel ? "sel" : "",
        "aria-selected": i === state.sel ? "true" : "false",
        onmousedown: (e) => e.preventDefault(), // keep the focus in the search field
        onclick: () => { state.sel = i; renderResults(); commit(h.word); },
      },
      el("span", { class: "w", text: h.word }),
      state.dicts > 1 && h.dicts > 1 ? el("span", { class: "n", text: String(h.dicts), title: `in ${h.dicts} dictionaries` }) : null,
    );
    results.append(li);
  });
  results.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
}

function moveSelection(by) {
  if (!state.hits.length) return;
  state.sel = Math.max(0, Math.min(state.hits.length - 1, state.sel + by));
  renderResults();
  clearTimeout(previewTimer);
  const w = state.hits[state.sel].word;
  previewTimer = setTimeout(() => show(w, false), 60);
}

// ── articles ─────────────────────────────────────────────────
async function show(word, push) {
  if (!state.ready) {
    state.pending = word;
    return;
  }
  const token = ++state.token;
  const arts = await invoke("lookup", { word });
  if (token !== state.token) return;
  if (!arts.length) {
    const hits = await invoke("search", { query: word, limit: 8 });
    if (token !== state.token) return;
    showNotFound(word, hits);
    return;
  }
  state.shown = word;
  wordTitle.textContent = word;
  articles.replaceChildren(
    ...arts.map((a) => {
      const card = el("article", { class: "article", style: `--dot: ${dictColor(a.dict_id)}` });
      card.append(el("div", { class: "dict", text: a.dict_name }));
      // the headword only when it says more than the title (another form, or other headwords)
      const head = a.headwords[0] || word;
      const same = head.toLowerCase() === wordTitle.textContent.toLowerCase();
      if (!same || a.headwords.length > 1) {
        const hw = el("h2", { class: "hw", text: head });
        if (a.headwords.length > 1) hw.append(" ", el("span", { class: "alt", text: a.headwords.slice(1).join(", ") }));
        card.append(hw);
      }
      const body = el("div", { class: "body" });
      body.innerHTML = a.html; // built by the engine: text escaped, only known tags and attributes
      card.append(body);
      return card;
    }),
  );
  articles.scrollTop = 0;
  if (push) {
    if (state.history[state.pos] !== word) {
      state.history = state.history.slice(0, state.pos + 1);
      state.history.push(word);
      state.pos = state.history.length - 1;
    }
    document.getElementById("app").classList.add("reading");
  }
  updateNav();
}

function commit(word) {
  clearTimeout(previewTimer);
  show(word, true);
}

function showNotFound(word, hits) {
  state.shown = null;
  wordTitle.textContent = "";
  const box = el("div", { class: "empty not-found" },
    el("h2", { text: `“${word}” isn’t in your dictionaries` }));
  if (hits.length) {
    box.append(el("p", { text: "Maybe one of these:" }));
    const s = el("div", { class: "suggest" });
    for (const h of hits) s.append(el("a", { href: "#", text: h.word, onclick: (e) => { e.preventDefault(); lookUp(h.word); } }));
    box.append(s);
  }
  articles.replaceChildren(box);
}

function showWelcome() {
  wordTitle.textContent = "";
  if (!state.ready) {
    articles.replaceChildren(el("div", { class: "empty" }, el("p", { text: "Opening the dictionaries…" })));
    return;
  }
  if (state.dicts === 0) {
    articles.replaceChildren(el("div", { class: "empty" },
      el("img", { src: "icon.svg", alt: "" }),
      el("h2", { text: "Welcome to Kosha" }),
      el("p", { text: "Add a folder with your dictionaries (ABBYY Lingvo .dsl or .dsl.dz). Kosha reads them where they are." }),
      el("button", { class: "btn primary", text: "Add folder…", onclick: addFolder }),
    ));
    return;
  }
  articles.replaceChildren(el("div", { class: "empty" },
    el("img", { src: "icon.svg", alt: "" }),
    el("p", { text: `Type a word to look it up in ${plural(state.dicts, "dictionary", "dictionaries")}.` }),
  ));
}

/** Look a word up from outside the search field (cross-reference, suggestion, shortcut). */
function lookUp(word) {
  listFor(word);
  commit(word);
}

/** Put a word in the search field (selected, so typing replaces it) and list its neighbours. */
function listFor(word) {
  q.value = word;
  q.select();
  $("clear").hidden = false;
  clearTimeout(previewTimer);
  invoke("search", { query: word, limit: 80 }).then((hits) => {
    if (q.value !== word) return;
    state.hits = hits;
    const exact = hits.findIndex((h) => !h.fuzzy && h.word.toLowerCase() === word.toLowerCase());
    state.sel = exact >= 0 ? exact : (hits.length ? 0 : -1);
    renderResults();
  });
}

articles.addEventListener("click", (e) => {
  const ref = e.target.closest("a.ref");
  if (ref) {
    e.preventDefault();
    lookUp(ref.dataset.word);
    return;
  }
  const url = e.target.closest("a.url, a.file");
  if (url) {
    e.preventDefault();
    invoke("plugin:opener|open_url", { url: url.href }).catch(() => {});
    return;
  }
  const sound = e.target.closest("button.sound");
  if (sound) play(sound);
});

let audio = null;
function play(button) {
  audio?.pause();
  document.querySelectorAll("button.sound.playing").forEach((b) => b.classList.remove("playing"));
  audio = new Audio(button.dataset.src);
  button.classList.add("playing");
  audio.onended = audio.onerror = () => button.classList.remove("playing");
  audio.play().catch(() => button.classList.remove("playing"));
}

// ── history ──────────────────────────────────────────────────
function go(by) {
  const p = state.pos + by;
  if (p < 0 || p >= state.history.length) return;
  state.pos = p;
  const w = state.history[p];
  listFor(w);
  show(w, false);
}
function updateNav() {
  $("back").disabled = state.pos <= 0;
  $("forward").disabled = state.pos >= state.history.length - 1;
}
$("back").onclick = () => go(-1);
$("forward").onclick = () => go(1);
$("back-list").onclick = () => { document.getElementById("app").classList.remove("reading"); q.focus(); };

// ── keyboard ─────────────────────────────────────────────────
q.addEventListener("input", runSearch);
q.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") { e.preventDefault(); moveSelection(1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); moveSelection(-1); }
  else if (e.key === "PageDown") { e.preventDefault(); moveSelection(10); }
  else if (e.key === "PageUp") { e.preventDefault(); moveSelection(-10); }
  else if (e.key === "Enter") {
    e.preventDefault();
    const h = state.hits[state.sel];
    commit(h ? h.word : q.value);
  }
});
$("clear").onclick = () => { q.value = ""; runSearch(); q.focus(); };

document.addEventListener("keydown", (e) => {
  const mod = e.ctrlKey || e.metaKey;
  if (e.key === "Escape") {
    if (!$("settings").hidden) { closeSettings(); return; }
    if (q.value) { q.value = ""; runSearch(); }
    q.focus();
    return;
  }
  if (mod && (e.key === "f" || e.key === "l" || e.key === "k")) { e.preventDefault(); q.focus(); q.select(); return; }
  if (mod && (e.key === "=" || e.key === "+")) { e.preventDefault(); size += 1; applySize(); return; }
  if (mod && e.key === "-") { e.preventDefault(); size -= 1; applySize(); return; }
  if (mod && e.key === "0") { e.preventDefault(); size = 17; applySize(); return; }
  if (mod && e.key === ",") { e.preventDefault(); openSettings(); return; }
  if ((e.altKey && e.key === "ArrowLeft") || (e.metaKey && e.key === "[")) { e.preventDefault(); go(-1); return; }
  if ((e.altKey && e.key === "ArrowRight") || (e.metaKey && e.key === "]")) { e.preventDefault(); go(1); return; }
  // typing anywhere goes to the search field
  const inField = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
  if (!inField && !mod && !e.altKey && e.key.length === 1 && $("settings").hidden) {
    q.focus();
    q.select();
  }
});

// ── dictionaries & folders ───────────────────────────────────
async function addFolder() {
  const path = await invoke("plugin:dialog|open", {
    options: { directory: true, multiple: false, title: "Folder with dictionaries" },
  });
  if (path) {
    await invoke("add_folder", { path }).catch((err) => alert(err));
    if (!$("settings").hidden) refreshSettings();
  }
}

function openSettings() {
  $("settings").hidden = false;
  refreshSettings();
}
function closeSettings() {
  $("settings").hidden = true;
  q.focus();
}

async function refreshSettings() {
  const lib = await invoke("library");
  $("loading").hidden = !lib.loading;
  $("folders").replaceChildren(...lib.folders.map((f) =>
    el("li", {},
      el("div", { class: "grow" }, el("div", { class: "title", text: f.split(/[\\/]/).filter(Boolean).pop() || f }), el("div", { class: "sub", text: f })),
      el("button", { class: "btn small", text: "Remove", onclick: () => invoke("remove_folder", { path: f }).then(refreshSettings) }),
    )));
  const n = lib.dicts.length;
  $("dicts").replaceChildren(...lib.dicts.map((d, i) => {
    const info = d.info;
    const langs = [info.index_language, info.contents_language].filter(Boolean).join(" → ");
    const parts = [langs, plural(info.articles, "article", "articles")];
    if (info.has_resources) parts.push("sounds & pictures");
    const toggle = el("input", { type: "checkbox", checked: d.enabled, "aria-label": `Use ${info.name}` });
    toggle.addEventListener("change", async () => {
      await invoke("set_enabled", { id: info.id, enabled: toggle.checked });
      li.classList.toggle("off", !toggle.checked);
      rerun();
    });
    const li = el("li", { class: d.enabled ? "" : "off", title: info.path },
      el("label", { class: "switch" }, toggle, el("span")),
      el("div", { class: "grow" }, el("div", { class: "title", text: info.name }), el("div", { class: "sub", text: parts.filter(Boolean).join(" · ") })),
      el("button", { class: "icon-btn", title: "Move up", "aria-label": "Move up", disabled: i === 0, onclick: () => move(info.id, -1) },
        svgIcon("M6 15l6-6 6 6")),
      el("button", { class: "icon-btn", title: "Move down", "aria-label": "Move down", disabled: i === n - 1, onclick: () => move(info.id, 1) },
        svgIcon("M6 9l6 6 6-6")),
    );
    return li;
  }));
  if (!lib.loading && n === 0) {
    $("dicts").replaceChildren();
  }
  $("failed").replaceChildren(...lib.failed.map((f) => el("div", { class: "err", text: `Can't read ${f.path}: ${f.error}` })));
}

function svgIcon(d) {
  const ns = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(ns, "svg");
  s.setAttribute("viewBox", "0 0 24 24");
  const p = document.createElementNS(ns, "path");
  p.setAttribute("d", d);
  s.append(p);
  return s;
}

async function move(id, by) {
  await invoke("move_dict", { id, by });
  refreshSettings();
  rerun();
}

function rerun() {
  if (q.value.trim()) runSearch();
  if (state.shown) show(state.shown, false);
}

$("open-settings").onclick = openSettings;
$("close-settings").onclick = closeSettings;
$("add-folder").onclick = addFolder;
$("rescan").onclick = () => invoke("rescan").then(refreshSettings);

// ── start ────────────────────────────────────────────────────
async function libraryChanged(ready) {
  state.ready = ready;
  const lib = await invoke("library");
  state.dicts = lib.dicts.filter((d) => d.enabled).length;
  $("dict-count").textContent = !ready
    ? "Opening dictionaries…"
    : lib.dicts.length
      ? plural(lib.dicts.length, "dictionary", "dictionaries")
      : "Add dictionaries";
  if (!$("settings").hidden) refreshSettings();
  if (!ready) return;
  if (state.pending) {
    const w = state.pending;
    state.pending = null;
    lookUp(w);
  } else if (q.value.trim()) {
    runSearch();
  } else if (state.shown) {
    show(state.shown, false);
  } else {
    showWelcome();
  }
}

listen("library", (e) => libraryChanged(e.payload));
listen("lookup", (e) => {
  if (state.ready) lookUp(e.payload);
  else state.pending = e.payload;
});

(async () => {
  state.pending = await invoke("first_word");
  const lib = await invoke("library");
  libraryChanged(!lib.loading);
  q.focus();
})();
