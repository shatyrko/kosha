// Settings: General (look and reading), Dictionaries (folders, on/off, order), Cards, About.

import { $, el, icon, invoke, plural, prefs, attempt, toast } from "./util.js";
import { draggable } from "./drag.js";
import { loadOnlineSettings, clearHistory, runSearch } from "./dictionary.js";
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
  ["online", "Online", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"],
  ["about", "About", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8h.01"],
];

function render() {
  const nav = $("settings").querySelector(".settings-nav");
  nav.replaceChildren(...SECTIONS.map(([id, label, ic]) =>
    el("button", { class: id === section ? "on" : "", onclick: () => { section = id; render(); if (id === "dictionaries") loadLibrary(); } },
      icon(ic, 18), el("span", { text: label }))));
  const body = $("settings").querySelector(".settings-body");
  const build = { general, dictionaries, cards, online, about }[section];
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
    el("h3", { text: "Search history" }),
    checkbox("Keep a search history", "history", true, () => runSearch(),
      "The words you open, newest first, shown under the search field while it is empty."),
    el("div", { class: "row-actions" },
      el("button", { class: "btn", text: "Clear the history", onclick: async () => { await clearHistory(); toast("Search history cleared"); } })),
  ];
}

// ── Dictionaries ─────────────────────────────────────────────
function dictionaries() {
  const out = [
    el("h3", { text: "Folders" }),
    el("p", { class: "hint" }, "Kosha finds dictionaries in these folders and their sub-folders (", el("code", { text: ".dsl" }), ", ",
      el("code", { text: ".dsl.dz" }), ", Apple ", el("code", { text: ".dictionary" }), ") and reads them where they are."),
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
    const handle = el("span", { class: "grip", title: "Drag to change the order", "aria-hidden": "true" }, icon("grip", 18));
    const li = el("li", { class: d.enabled ? "" : "off", title: info.path, "data-id": info.id },
      handle,
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
    dragRow(li, handle, info.name);
    return li;
  })));
  for (const f of store.failed) out.push(el("div", { class: "err", text: `Can't read ${f.path}: ${f.error}` }));
  return out;
}

// ── dragging a dictionary to another place in the list ───────
// Only from the grip, so the switch and the arrows keep working as they are.
function dragRow(li, handle, name) {
  draggable(handle, {
    label: () => name,
    iconName: "book",
    start: () => li.classList.add("dragging"),
    finish: () => li.classList.remove("dragging"),
    clear: clearMarks,
    over: (x, y, under) => {
      const row = under.closest(".rows li[data-id]");
      if (!row || row.parentElement !== li.parentElement) return null;
      const r = row.getBoundingClientRect();
      const after = y > r.top + r.height / 2;
      row.classList.add(after ? "drop-after" : "drop-before");
      return { id: row.dataset.id, after };
    },
    drop: async ({ id, after }) => {
      const moved = li.dataset.id;
      if (id === moved) return;
      const order = [...li.parentElement.children].map((x) => x.dataset.id).filter((x) => x !== moved);
      order.splice(order.indexOf(id) + (after ? 1 : 0), 0, moved);
      await attempt(invoke("set_order", { order }));
      await loadLibrary();
      onChange();
    },
  });
}

function clearMarks() {
  document.querySelectorAll(".drop-before, .drop-after").forEach((x) => x.classList.remove("drop-before", "drop-after"));
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

// ── Online ───────────────────────────────────────────────────
const LANGS = [["ru", "Russian"], ["uk", "Ukrainian"], ["en", "English"], ["de", "German"], ["fr", "French"],
  ["es", "Spanish"], ["it", "Italian"], ["pt", "Portuguese"], ["pl", "Polish"], ["zh", "Chinese"], ["ja", "Japanese"]];
let onlineDraft = null;

function online() {
  const box = el("div", {}, el("p", { class: "hint", text: "…" }));
  invoke("online_settings").then((o) => { onlineDraft = o; box.replaceChildren(...onlineRows()); });
  return [
    el("p", { class: "hint", text: "After the dictionaries, Kosha can ask online services about the word or phrase you look up. "
      + "What you look up is then sent to them. Each can be switched off." }),
    box,
  ];
}

async function saveOnline() {
  await attempt(invoke("set_online_settings", { online: onlineDraft }));
  await loadOnlineSettings();
}

function onlineRows() {
  const o = onlineDraft;
  const langSelect = (key) => {
    const sel = el("select", { "aria-label": key });
    for (const [v, n] of LANGS) sel.append(el("option", { value: v, text: n }));
    sel.value = o[key];
    sel.onchange = () => { o[key] = sel.value; saveOnline(); };
    return sel;
  };
  const toggle = (key, text, hint) => {
    const b = el("input", { type: "checkbox" });
    b.checked = o[key];
    b.onchange = () => { o[key] = b.checked; saveOnline(); };
    return el("div", { class: "setting" }, el("label", { class: "check" }, b, text), hint ? el("p", { class: "hint", text: hint }) : null);
  };
  const keyRow = (key, source, getUrl, getText) => {
    const input = el("input", { type: "password", class: "key", value: o[key], placeholder: "API key", spellcheck: "false", autocomplete: "off" });
    input.onchange = () => { o[key] = input.value.trim(); saveOnline(); };
    const result = el("span", { class: "sub" });
    return el("div", { class: "setting key-row" },
      input,
      el("button", { class: "btn small", text: "Test", onclick: async () => {
        o[key] = input.value.trim();
        await saveOnline();
        result.textContent = "…";
        try {
          const f = await invoke("online_lookup", { source, text: "Good morning" });
          result.textContent = f ? `Works: “Good morning” → “${f.text}”` : "No answer: is the key filled in?";
        } catch (e) {
          result.textContent = String(e);
        }
      } }),
      el("button", { class: "link-btn", text: getText, onclick: () => invoke("plugin:opener|open_url", { url: getUrl }).catch(() => {}) }),
      result);
  };
  return [
    row("Your language", langSelect("native"), "Translations go into your language, and from it into the one you learn."),
    row("The language you learn", langSelect("learning")),
    el("h3", { text: "Wikipedia" }),
    toggle("wikipedia", "Show the Wikipedia article", "Its summary and picture, from the English or Russian Wikipedia (by the script of the word)."),
    el("h3", { text: "DeepL" }),
    toggle("deepl", "Translate with DeepL", "Needs your DeepL API key; the free plan has 500,000 characters a month (keys ending in “:fx”)."),
    keyRow("deepl_key", "deepl", "https://www.deepl.com/pro-api", "Get a free key"),
    el("h3", { text: "Google Translate" }),
    toggle("google", "Translate with Google", "Needs a Google Cloud Translation API key (Google Cloud console; billing must be set up, with a monthly free allowance). Without one, the “Google Translate” button opens the website instead."),
    keyRow("google_key", "google", "https://console.cloud.google.com/apis/library/translate.googleapis.com", "Where to get a key"),
    el("p", { class: "hint", text: "Keys are kept in Kosha's settings file on this computer only." }),
  ];
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
