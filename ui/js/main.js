// Kosha's window: puts the parts together (dictionary, cards, study, settings) and handles the
// keys that work everywhere. Talks to the Rust side through Tauri; no framework, no build step.

import { $, invoke, listen, plural, prefs, closeMenu, menuOpen } from "./util.js";
import { store, on, loadDeck, loadLibrary, setReady } from "./store.js";
import * as dict from "./dictionary.js";
import * as cards from "./cards.js";
import * as study from "./study.js";
import * as settings from "./settings.js";

settings.applyLook();

// scroll bars show while an element scrolls, and fade a moment after it stops (style.css)
const scrollTimers = new WeakMap();
document.addEventListener("scroll", (e) => {
  const t = e.target === document ? document.scrollingElement : e.target;
  if (!(t instanceof Element)) return;
  t.classList.add("scrolling");
  clearTimeout(scrollTimers.get(t));
  scrollTimers.set(t, setTimeout(() => {
    t.classList.remove("scrolling");
    // WebKit keeps showing the old bar until it is rebuilt: drop it for a moment
    const top = t.scrollTop;
    t.style.overflow = "hidden";
    void t.offsetHeight;
    t.style.overflow = "";
    t.scrollTop = top;
  }, 900));
}, true);

// ── Dictionary / Cards ───────────────────────────────────────
let mode = prefs.get("mode", "dictionary");

function setMode(m) {
  mode = m;
  prefs.set("mode", m);
  $("app").dataset.mode = m;
  $("tab-dict").classList.toggle("on", m === "dictionary");
  $("tab-cards").classList.toggle("on", m === "cards");
  $("tab-dict").setAttribute("aria-selected", String(m === "dictionary"));
  $("tab-cards").setAttribute("aria-selected", String(m === "cards"));
  if (m === "cards") cards.render();
  (m === "cards" ? $("card-filter") : $("q")).focus();
}

$("tab-dict").onclick = () => setMode("dictionary");
$("tab-cards").onclick = () => setMode("cards");
$("open-settings").onclick = () => settings.open();

on("deck", () => {
  const n = store.deck.cards.length;
  $("cards-badge").textContent = n ? String(n) : "";
});
on("library", () => {
  $("dict-status").textContent = !store.ready
    ? "Opening dictionaries…"
    : store.dicts.length
      ? plural(store.dicts.length, "dictionary", "dictionaries")
      : "No dictionaries yet";
});
$("dict-status").onclick = () => settings.open("dictionaries");

dict.init({ addFolder: settings.addFolder });
cards.init({ lookUp: (w) => { setMode("dictionary"); dict.lookUp(w); } });
settings.init({ onChange: () => dict.rerun() });
settings.initDom();

// ── keys that work everywhere ────────────────────────────────
document.addEventListener("keydown", (e) => {
  const mod = e.ctrlKey || e.metaKey;
  if (study.isOpen() || cards.dialogOpen() || document.querySelector(".dialog-back:not(#card-dialog)")) return; // they have their own keys
  if (e.key === "Escape") {
    if (menuOpen()) { closeMenu(); return; }
    if (settings.isOpen()) { settings.close(); return; }
    if (mode === "dictionary") dict.clearSearch();
    else if ($("card-filter").value) { $("card-filter").value = ""; cards.render(); }
    (mode === "cards" ? $("card-filter") : $("q")).focus();
    return;
  }
  if (mod && e.key === "1") { e.preventDefault(); setMode("dictionary"); return; }
  if (mod && e.key === "2") { e.preventDefault(); setMode("cards"); return; }
  if (mod && e.key === ",") { e.preventDefault(); settings.isOpen() ? settings.close() : settings.open(); return; }
  if (mod && (e.key === "=" || e.key === "+")) { e.preventDefault(); settings.changeSize(1); return; }
  if (mod && e.key === "-") { e.preventDefault(); settings.changeSize(-1); return; }
  if (mod && e.key === "0") { e.preventDefault(); settings.changeSize(0); return; }
  if (settings.isOpen()) return;
  if (mod && (e.key === "f" || e.key === "l" || e.key === "k")) {
    e.preventDefault();
    const f = mode === "cards" ? $("card-filter") : $("q");
    f.focus();
    f.select();
    return;
  }
  if (mode === "dictionary") {
    if (mod && e.key === "s") { e.preventDefault(); dict.quickSave(); return; }
    if ((e.altKey && e.key === "ArrowLeft") || (e.metaKey && e.key === "[")) { e.preventDefault(); dict.go(-1); return; }
    if ((e.altKey && e.key === "ArrowRight") || (e.metaKey && e.key === "]")) { e.preventDefault(); dict.go(1); return; }
    // typing anywhere goes to the search field
    const inField = e.target.matches?.("input, textarea, select");
    if (!inField && !mod && !e.altKey && e.key.length === 1) {
      $("q").focus();
      $("q").select();
    }
  }
});

// ── the dictionaries opening (at start, after a folder change) ─
async function libraryChanged(ready) {
  setReady(ready);
  await loadLibrary();
  dict.libraryChanged();
}

listen("library", (e) => libraryChanged(e.payload));
listen("lookup", (e) => {
  setMode("dictionary");
  if (store.ready) dict.lookUp(e.payload);
  else dict.setPending(e.payload);
});

(async () => {
  await loadDeck();
  const first = await invoke("first_word");
  if (first) {
    mode = "dictionary";
    dict.setPending(first);
  }
  setMode(mode);
  const lib = await invoke("library");
  await libraryChanged(!lib.loading);
})();
