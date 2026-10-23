// The dictionary: search field, headword list, articles, history, saving words as cards.

import { $, el, icon, invoke, dictColor, plural, prefs, menu, toast } from "./util.js";
import { store, on, tree, savedCards, saveCard, folderPath, newFolder, deckCommand, folder } from "./store.js";

const q = $("q");
const results = $("results");
const articles = $("articles");
const wordTitle = $("word");

const state = {
  hits: [],
  sel: -1,
  shown: null,      // word whose articles are on screen
  arts: [],         // those articles
  history: [],
  pos: -1,
  pending: null,    // word to show once the dictionaries are open
  token: 0,         // newest request; older answers are dropped
};

let previewTimer = 0;
let onAddFolder = () => {};

export function init({ addFolder }) {
  onAddFolder = addFolder;
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
  $("back").onclick = () => go(-1);
  $("forward").onclick = () => go(1);
  $("back-list").onclick = () => { $("app").classList.remove("reading"); q.focus(); };
  articles.addEventListener("click", articleClick);
  // the saved marks follow the deck
  on("deck", () => refreshSaveButtons());
}

// ── search ───────────────────────────────────────────────────
export async function runSearch() {
  const text = q.value;
  $("clear").hidden = text === "";
  if (!text.trim()) {
    state.hits = [];
    state.sel = -1;
    renderResults();
    if (!state.shown) showWelcome();
    return;
  }
  if (!store.ready) {
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

function enabledCount() {
  return store.dicts.filter((d) => d.enabled).length;
}

function renderResults() {
  results.replaceChildren();
  if (!store.ready && q.value.trim()) {
    results.append(el("li", { class: "note", text: "Opening the dictionaries…" }));
    return;
  }
  let fuzzyHeader = false;
  const many = enabledCount() > 1;
  state.hits.forEach((h, i) => {
    if (h.fuzzy && !fuzzyHeader) {
      fuzzyHeader = true;
      results.append(el("li", { class: "group", text: i === 0 ? "Did you mean" : "Similar" }));
    }
    results.append(el(
      "li",
      {
        role: "option",
        class: i === state.sel ? "sel" : "",
        "aria-selected": i === state.sel ? "true" : "false",
        onmousedown: (e) => e.preventDefault(), // keep the focus in the search field
        onclick: () => { state.sel = i; renderResults(); commit(h.word); },
      },
      el("span", { class: "w", text: h.word }),
      many && h.dicts > 1 ? el("span", { class: "n", text: String(h.dicts), title: `in ${h.dicts} dictionaries` }) : null,
    ));
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
export async function show(word, push) {
  if (!store.ready) {
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
  state.arts = arts;
  wordTitle.textContent = word;
  articles.replaceChildren(...arts.map((a) => articleCard(a, word)));
  articles.scrollTop = 0;
  if (push) {
    if (state.history[state.pos] !== word) {
      state.history = state.history.slice(0, state.pos + 1);
      state.history.push(word);
      state.pos = state.history.length - 1;
    }
    $("app").classList.add("reading");
  }
  updateNav();
  if (prefs.get("autoplay", false)) articles.querySelector("button.sound")?.click();
}

function articleCard(a, word) {
  const card = el("article", { class: "article", style: `--dot: ${dictColor(a.dict_id)}`, "data-dict": a.dict_id });
  const save = el("button", { class: "save-btn", "data-dict": a.dict_id });
  save.addEventListener("click", () => saveMenu(save, word, a.dict_id));
  card.append(el("div", { class: "article-head" }, el("div", { class: "dict", text: a.dict_name }), save));
  // the headword only when it says more than the title (another form, or other headwords)
  const head = a.headwords[0] || word;
  const same = head.toLowerCase() === word.toLowerCase();
  if (!same || a.headwords.length > 1) {
    const hw = el("h2", { class: "hw", text: head });
    if (a.headwords.length > 1) hw.append(" ", el("span", { class: "alt", text: a.headwords.slice(1).join(", ") }));
    card.append(hw);
  }
  const body = el("div", { class: "body" });
  body.innerHTML = a.html; // built by the engine: text escaped, only known tags and attributes
  card.append(body);
  setSaveButton(save, word, a.dict_id);
  return card;
}

function setSaveButton(btn, word, dictId) {
  const saved = savedCards(word, dictId);
  btn.replaceChildren(icon("bookmark", 16), el("span", { text: saved.length ? "Saved" : "Save" }));
  btn.classList.toggle("saved", saved.length > 0);
  btn.title = saved.length
    ? `In your cards: ${saved.map((c) => (c.folder ? folderPath(c.folder) : "Unsorted")).join(", ")}`
    : "Save to your word cards (Ctrl+S)";
}

function refreshSaveButtons() {
  if (!state.shown) return;
  articles.querySelectorAll(".save-btn").forEach((b) => setSaveButton(b, state.shown, b.dataset.dict));
}

/** The folder menu for saving a word; also shows and removes existing cards of it. */
function saveMenu(anchor, word, dictId) {
  const saved = savedCards(word, dictId);
  const inFolder = new Set(saved.map((c) => c.folder || null));
  const items = [];
  if (store.deckError) {
    toast(store.deckError, "error");
    return;
  }
  if (saved.length) {
    items.push({ header: "In your cards" });
    for (const c of saved) {
      items.push({
        label: `Remove from ${c.folder ? folderPath(c.folder) : "Unsorted"}`,
        icon: "trash",
        danger: true,
        onclick: () => deckCommand("card_delete", { id: c.id }).then((d) => d && toast(`Removed “${word}”`)),
      });
    }
    items.push("-");
  }
  items.push({ header: saved.length ? "Also save to" : "Save to" });
  items.push({ label: "Unsorted", icon: "inbox", checked: inFolder.has(null), onclick: () => saveCard(word, dictId, null) });
  for (const { folder: f, depth } of tree()) {
    items.push({ label: f.name, icon: "folder", indent: depth, checked: inFolder.has(f.id), onclick: () => saveCard(word, dictId, f.id) });
  }
  items.push("-");
  items.push({
    label: "New folder…",
    icon: "plus",
    onclick: async () => {
      const f = await newFolder(null);
      if (f) saveCard(word, dictId, f.id);
    },
  });
  menu(anchor, items);
}

/** Ctrl+S: the first article of the word on screen into the folder used last. */
export function quickSave() {
  const a = state.arts[0];
  if (!state.shown || !a) return;
  let target = prefs.get("lastFolder", null);
  if (target && !folder(target)) target = null;
  saveCard(state.shown, a.dict_id, target);
}

export function commit(word) {
  clearTimeout(previewTimer);
  show(word, true);
}

function showNotFound(word, hits) {
  state.shown = null;
  state.arts = [];
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

export function showWelcome() {
  wordTitle.textContent = "";
  if (!store.ready) {
    articles.replaceChildren(el("div", { class: "empty" }, el("p", { text: "Opening the dictionaries…" })));
    return;
  }
  if (store.dicts.length === 0) {
    articles.replaceChildren(el("div", { class: "empty" },
      el("img", { src: "icon.svg", alt: "" }),
      el("h2", { text: "Welcome to Kosha" }),
      el("p", { text: "Add a folder with your dictionaries (ABBYY Lingvo .dsl or .dsl.dz). Kosha reads them where they are." }),
      el("button", { class: "btn primary", text: "Add folder…", onclick: () => onAddFolder() }),
    ));
    return;
  }
  articles.replaceChildren(el("div", { class: "empty" },
    el("img", { src: "icon.svg", alt: "" }),
    el("p", { text: `Type a word to look it up in ${plural(enabledCount(), "dictionary", "dictionaries")}.` }),
  ));
}

/** Look a word up from outside the search field (cross-reference, suggestion, card, shortcut). */
export function lookUp(word) {
  if (!store.ready) {
    state.pending = word;
    return;
  }
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

function articleClick(e) {
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
}

let audio = null;
export function play(button) {
  audio?.pause();
  document.querySelectorAll("button.sound.playing").forEach((b) => b.classList.remove("playing"));
  audio = new Audio(button.dataset.src);
  button.classList.add("playing");
  audio.onended = audio.onerror = () => button.classList.remove("playing");
  audio.play().catch(() => button.classList.remove("playing"));
}

// ── history ──────────────────────────────────────────────────
export function go(by) {
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

/** The dictionaries were opened (or changed): show what is pending or refresh what is shown. */
export function libraryChanged() {
  if (!store.ready) return;
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

export function rerun() {
  if (q.value.trim()) runSearch();
  if (state.shown) show(state.shown, false);
}

export function setPending(word) {
  state.pending = word;
}

export function clearSearch() {
  if (q.value) {
    q.value = "";
    runSearch();
  }
}
