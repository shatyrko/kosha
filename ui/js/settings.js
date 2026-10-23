// Settings: General (look and reading), Dictionaries (folders, on/off, order), Cards, About.

import { $, el, icon, invoke, plural, prefs, attempt, toast } from "./util.js";
import { store, on, loadLibrary } from "./store.js";

let section = "general";
let onChange = () => {};

// ── look: applied at start and whenever changed ──────────────
const dark = window.matchMedia("(prefers-color-scheme: dark)");

export function applyLook() {
  const theme = prefs.get("theme", "system");
  const resolved = theme === "system" ? (dark.matches ? "dark" : "light") : theme;
  document.documentElement.dataset.theme = resolved;
  const size = Math.min(28, Math.max(12, Number(prefs.get("size", 17)) || 17));
  document.documentElement.style.setProperty("--article-size", `${size}px`);
  document.body.classList.toggle("hide-secondary", !prefs.get("secondary", true));
}
dark.addEventListener("change", applyLook);

export function changeSize(by) {
  const size = by === 0 ? 17 : Math.min(28, Math.max(12, (Number(prefs.get("size", 17)) || 17) + by));
  prefs.set("size", size);
  applyLook();
  if (isOpen() && section === "general") render();
}

export function init(opts) {
  onChange = opts.onChange;
  on("library", () => { if (isOpen() && section === "dictionaries") render(); });
  on("deck", () => { if (isOpen() && section === "cards") render(); });
}

export function isOpen() {
  return !$("settings").hidden;
}

export function open(which) {
  if (which) section = which;
  $("settings").hidden = false;
  render();
  if (section === "dictionaries") loadLibrary();
  $("settings").querySelector(".settings-nav button.on")?.focus();
}

export function close() {
  $("settings").hidden = true;
  $("q").focus();
}

const SECTIONS = [
  ["general", "General", "gear"],
  ["dictionaries", "Dictionaries", "book"],
  ["cards", "Cards", "stack"],
  ["about", "About", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8h.01"],
];

function render() {
  const nav = $("settings").querySelector(".settings-nav");
  nav.replaceChildren(...SECTIONS.map(([id, label, ic]) =>
    el("button", { class: id === section ? "on" : "", onclick: () => { section = id; render(); if (id === "dictionaries") loadLibrary(); } },
      icon(ic, 18), el("span", { text: label }))));
  const body = $("settings").querySelector(".settings-body");
  const build = { general, dictionaries, cards, about }[section];
  body.replaceChildren(el("h2", { text: SECTIONS.find((s) => s[0] === section)[1] }), ...build().filter(Boolean));
}

function seg(options, value, onpick, label) {
  return el("div", { class: "seg", role: "radiogroup", "aria-label": label },
    ...options.map(([v, text]) => el("button", {
      class: v === value ? "on" : "",
      role: "radio",
      "aria-checked": String(v === value),
      onclick: () => onpick(v),
    }, text)));
}

function checkbox(text, key, fallback, after, hint) {
  const b = el("input", { type: "checkbox" });
  b.checked = prefs.get(key, fallback);
  b.onchange = () => { prefs.set(key, b.checked); after?.(); };
  return el("div", { class: "setting" }, el("label", { class: "check" }, b, text), hint ? el("p", { class: "hint", text: hint }) : null);
}

function row(label, control, hint) {
  return el("div", { class: "setting row" },
    el("div", { class: "grow" }, el("div", { class: "label", text: label }), hint ? el("p", { class: "hint", text: hint }) : null),
    control);
}

// ── General ──────────────────────────────────────────────────
function general() {
  const size = Number(prefs.get("size", 17)) || 17;
  const slider = el("input", { type: "range", min: "12", max: "28", step: "1", value: String(size), "aria-label": "Text size" });
  const out = el("span", { class: "value", text: `${size}px` });
  slider.oninput = () => { prefs.set("size", Number(slider.value)); out.textContent = `${slider.value}px`; applyLook(); };
  return [
    row("Appearance", seg([["system", "System"], ["light", "Light"], ["dark", "Dark"]], prefs.get("theme", "system"),
      (v) => { prefs.set("theme", v); applyLook(); render(); }, "Appearance")),
    row("Text size in articles", el("div", { class: "slider" }, el("span", { class: "a-small", text: "A" }), slider, el("span", { class: "a-big", text: "A" }), out),
      "Also Ctrl+= and Ctrl+− anywhere, Ctrl+0 for the standard size."),
    el("p", { class: "sample", style: `font-size: ${size}px` },
      el("b", { text: "apple " }), el("span", { class: "tr", text: "ˈæp(ə)l" }), " ", el("abbr", { class: "p", text: "n" }), " ",
      el("span", { class: "trn", text: "яблоко" }), " — ", el("span", { class: "ex", text: "an apple a day" })),
    el("h3", { text: "Reading" }),
    checkbox("Show secondary text", "secondary", true, applyLook, "Parts a dictionary marks as less important, shown smaller and dimmer."),
    checkbox("Play the pronunciation when an article opens", "autoplay", false, null, "For dictionaries with sound files."),
  ];
}

// ── Dictionaries ─────────────────────────────────────────────
function dictionaries() {
  const out = [
    el("h3", { text: "Folders" }),
    el("p", { class: "hint" }, "Kosha finds ", el("code", { text: ".dsl" }), " and ", el("code", { text: ".dsl.dz" }),
      " dictionaries in these folders and their sub-folders, and reads them where they are."),
  ];
  const folders = el("ul", { class: "rows" }, ...store.folders.map((f) =>
    el("li", {},
      icon("folder", 18),
      el("div", { class: "grow" }, el("div", { class: "title", text: f.split(/[\\/]/).filter(Boolean).pop() || f }), el("div", { class: "sub", text: f })),
      el("button", { class: "btn small", text: "Remove", onclick: async () => { await attempt(invoke("remove_folder", { path: f })); loadLibrary(); } }))));
  out.push(folders);
  out.push(el("div", { class: "row-actions" },
    el("button", { class: "btn primary", text: "Add folder…", onclick: addFolder }),
    el("button", { class: "btn", text: "Look again", title: "Find new or changed dictionaries", onclick: async () => { await invoke("rescan"); loadLibrary(); } })));

  out.push(el("h3", {}, "Dictionaries", store.loading ? el("span", { class: "spinner" }) : null));
  out.push(el("p", { class: "hint", text: "Articles are shown in this order. Switch a dictionary off to leave it out of search." }));
  if (!store.loading && !store.dicts.length) {
    out.push(el("p", { class: "hint", text: store.folders.length ? "No dictionaries found in these folders." : "Add a folder first." }));
  }
  const n = store.dicts.length;
  out.push(el("ul", { class: "rows" }, ...store.dicts.map((d, i) => {
    const info = d.info;
    const langs = [info.index_language, info.contents_language].filter(Boolean).join(" → ");
    const parts = [langs, plural(info.articles, "article", "articles")];
    if (info.has_resources) parts.push("sounds & pictures");
    const toggle = el("input", { type: "checkbox", "aria-label": `Use ${info.name}` });
    toggle.checked = d.enabled;
    const li = el("li", { class: d.enabled ? "" : "off", title: info.path },
      el("label", { class: "switch" }, toggle, el("span")),
      el("div", { class: "grow" }, el("div", { class: "title", text: info.name }), el("div", { class: "sub", text: parts.filter(Boolean).join(" · ") })),
      el("button", { class: "icon-btn", title: "Move up", "aria-label": "Move up", disabled: i === 0, onclick: () => move(info.id, -1) }, icon("up")),
      el("button", { class: "icon-btn", title: "Move down", "aria-label": "Move down", disabled: i === n - 1, onclick: () => move(info.id, 1) }, icon("down")));
    toggle.addEventListener("change", async () => {
      await attempt(invoke("set_enabled", { id: info.id, enabled: toggle.checked }));
      li.classList.toggle("off", !toggle.checked);
      await loadLibrary();
      onChange();
    });
    return li;
  })));
  for (const f of store.failed) out.push(el("div", { class: "err", text: `Can't read ${f.path}: ${f.error}` }));
  return out;
}

async function move(id, by) {
  await attempt(invoke("move_dict", { id, by }));
  await loadLibrary();
  onChange();
}

export async function addFolder() {
  const path = await attempt(invoke("plugin:dialog|open", {
    options: { directory: true, multiple: false, title: "Folder with dictionaries" },
  }));
  if (path) {
    const ok = await attempt(invoke("add_folder", { path }));
    if (ok !== null) toast("Opening the dictionaries…");
    loadLibrary();
  }
}

// ── Cards ────────────────────────────────────────────────────
function cards() {
  const all = store.deck.cards;
  const known = all.filter((c) => c.known).length;
  const reviews = all.reduce((n, c) => n + c.reviews, 0);
  return [
    el("div", { class: "stats" },
      stat(all.length, "cards"), stat(store.deck.folders.length, "folders"), stat(known, "known"), stat(reviews, "answers in study")),
    el("h3", { text: "Study" }),
    row("Front of the card", seg([["word", "The word"], ["meaning", "The meaning"]], prefs.get("studyFront", "word"),
      (v) => { prefs.set("studyFront", v); render(); }, "Front of the card"),
    "With the meaning first, the word is hidden in the article and you recall it."),
    checkbox("Shuffle the cards", "studyShuffle", false),
    checkbox("Only cards not marked “Got it” yet", "studyOnlyNew", false),
    el("p", { class: "hint" }, "In study: ", el("kbd", { text: "Space" }), " flips, ", el("kbd", { text: "←" }), " ", el("kbd", { text: "→" }),
      " move, ", el("kbd", { text: "1" }), " again, ", el("kbd", { text: "2" }), " got it, ", el("kbd", { text: "Esc" }), " ends."),
    store.deckError ? el("div", { class: "err", text: store.deckError }) : null,
  ];
}

function stat(n, label) {
  return el("div", { class: "stat" }, el("b", { text: String(n) }), el("span", { text: label }));
}

// ── About ────────────────────────────────────────────────────
function about() {
  const box = el("div", { class: "about-paths" }, el("p", { class: "hint", text: "…" }));
  invoke("about").then((a) => {
    $("about-version").textContent = `Version ${a.version}`;
    box.replaceChildren(
      pathRow("Settings", a.settings), pathRow("Word cards", a.cards), pathRow("Indexes (can be deleted, rebuilt on start)", a.cache));
  });
  const keys = [
    ["Ctrl+F / Ctrl+L", "search"], ["↑ ↓ Enter", "choose a word"], ["Alt+← / Alt+→", "back / forward"],
    ["Ctrl+S", "save the word as a card"], ["Ctrl+1 / Ctrl+2", "dictionary / cards"], ["Ctrl+,", "settings"],
    ["Ctrl+= / Ctrl+−", "text size"], ["Esc", "clear the search, close"],
  ];
  return [
    el("div", { class: "about-head" },
      el("img", { src: "icon.svg", alt: "" }),
      el("div", {}, el("div", { class: "about-name", text: "Kosha" }), el("div", { id: "about-version", class: "sub", text: "" }),
        el("div", { class: "sub", text: "Dictionaries read where they are. Free software (GPL-3.0-or-later)." }),
        el("a", { href: "#", class: "url", text: "github.com/shatyrko/kosha", onclick: (e) => { e.preventDefault(); invoke("plugin:opener|open_url", { url: "https://github.com/shatyrko/kosha" }).catch(() => {}); } }))),
    el("h3", { text: "Where your data is" }),
    box,
    el("h3", { text: "Keyboard" }),
    el("table", { class: "keys" }, ...keys.map(([k, what]) => el("tr", {}, el("td", {}, el("kbd", { text: k })), el("td", { text: what })))),
  ];
}

function pathRow(label, path) {
  return el("div", { class: "path-row" }, el("div", { class: "label", text: label }), el("code", { text: path }));
}

export function initDom() {
  $("close-settings").onclick = close;
}
