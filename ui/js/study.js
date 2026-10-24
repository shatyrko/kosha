// Study mode: the cards one at a time, large; flip to see the other side, arrows to move,
// "Again" / "Got it" to mark how it went.

import { $, el, icon, plural, prefs } from "./util.js";
import { store, deckCommand } from "./store.js";
import { play } from "./dictionary.js";

let s = null; // {all, list, i, flipped, title, got: Set, again: Set}

function shuffle(a) {
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

const opt = {
  get front() { return prefs.get("studyFront", "word"); },
  get shuffle() { return prefs.get("studyShuffle", false); },
  get onlyNew() { return prefs.get("studyOnlyNew", false); },
};

/** Study these cards (in this order), starting at `index`. */
export function start(cards, title, index = 0) {
  const ids = cards.map((c) => c.id);
  s = { all: ids, title, i: 0, flipped: false, got: new Set(), again: new Set(), startAt: ids[index] };
  arrange();
  open();
}

/** The order to study in, from the options: only unknown, shuffled. */
function arrange(keepCurrent) {
  const current = keepCurrent ? s.list?.[s.i] : s.startAt;
  let list = [...s.all];
  if (opt.onlyNew) {
    const unknown = list.filter((id) => !card(id)?.known);
    if (unknown.length) list = unknown;
  }
  if (opt.shuffle) shuffle(list);
  s.list = list;
  const at = current ? list.indexOf(current) : -1;
  if (opt.shuffle && at > 0 && !keepCurrent) {
    // start with the chosen card even when shuffled
    [list[0], list[at]] = [list[at], list[0]];
    s.i = 0;
  } else {
    s.i = Math.max(0, at);
  }
  s.flipped = false;
}

const card = (id) => store.deck.cards.find((c) => c.id === id);

function open() {
  $("study-view")?.remove();
  const v = el("div", { id: "study-view", role: "dialog", "aria-label": "Study", tabindex: "-1" });
  document.body.append(v);
  render();
  v.focus();
}

export function close() {
  $("study-view")?.remove();
  s = null;
  document.getElementById("q")?.focus();
}

export function isOpen() {
  return !!s;
}

function toggle(name, label, key, onchange) {
  const b = el("input", { type: "checkbox" });
  b.checked = prefs.get(key, false);
  b.onchange = () => { prefs.set(key, b.checked); onchange(); };
  return el("label", { class: "check small", title: label }, b, name);
}

/** Hide the word in an article (for "start from the meaning"). */
function mask(node, words) {
  const ws = words.filter((w) => w && w.length > 1).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  if (!ws.length) return;
  const re = new RegExp(`(${ws.join("|")})`, "giu");
  const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
  const texts = [];
  while (walker.nextNode()) texts.push(walker.currentNode);
  for (const t of texts) {
    if (re.test(t.nodeValue)) {
      re.lastIndex = 0;
      t.nodeValue = t.nodeValue.replace(re, "…");
    }
  }
}

function render() {
  const v = $("study-view");
  if (!v || !s) return;
  const total = s.list.length;
  if (s.i >= total) return finished(v);
  const c = card(s.list[s.i]);
  if (!c) { s.list.splice(s.i, 1); return render(); }
  const front = opt.front;

  const wordFace = el("div", { class: "face-word" },
    el("div", { class: "big-word", text: c.word }),
    c.headword && c.headword.toLowerCase() !== c.word.toLowerCase() ? el("div", { class: "face-hw", text: c.headword }) : null,
    el("div", { class: "face-dict", text: c.dict_name }));
  const meaning = (hide) => {
    const body = el("div", { class: `face-article${hide ? " masked" : ""}` });
    body.innerHTML = c.html; // a copy of the engine's article HTML
    if (hide) mask(body, [c.word, c.headword]);
    const parts = [body];
    // the note may well contain the word: on the answer side only
    if (c.note && !hide) parts.push(el("div", { class: "face-note", text: c.note }));
    return parts;
  };
  const frontFace = front === "meaning"
    ? el("div", { class: "face front" }, el("div", { class: "face-hint", text: "Which word is it?" }), ...meaning(true))
    : el("div", { class: "face front" }, wordFace);
  const backFace = el("div", { class: "face back" },
    el("div", { class: "back-word", text: c.word }),
    ...meaning(false));

  const flip = el("div", { class: `flip${s.flipped ? " flipped" : ""}`, onclick: (e) => {
    const sound = e.target.closest("button.sound");
    if (sound) { e.stopPropagation(); play(sound); return; }
    if (e.target.closest("a")) { e.preventDefault(); return; }
    turn();
  } }, frontFace, backFace);

  const progress = el("div", { class: "study-progress" }, el("span", { style: `width: ${(s.i / total) * 100}%` }));
  v.replaceChildren(
    el("header", { class: "study-bar" },
      el("div", { class: "grow" }, el("div", { class: "study-title", text: s.title }), el("div", { class: "sub", text: `${s.i + 1} of ${total}${s.got.size || s.again.size ? ` · ✓ ${s.got.size}  ↺ ${s.again.size}` : ""}` })),
      el("div", { class: "study-opts" },
        el("div", { class: "seg small", role: "radiogroup", "aria-label": "Front of the card" },
          ...[["word", "Word first"], ["meaning", "Meaning first"]].map(([val, label]) =>
            el("button", { class: front === val ? "on" : "", role: "radio", "aria-checked": String(front === val), onclick: () => { prefs.set("studyFront", val); s.flipped = false; render(); } }, label))),
        toggle("Shuffle", "Mix the order", "studyShuffle", () => { arrange(true); render(); }),
        toggle("Only new", "Skip cards marked “Got it”", "studyOnlyNew", () => { arrange(true); render(); })),
      el("button", { class: "icon-btn close-btn", title: "Close (Esc)", "aria-label": "Close", onclick: close }, icon("close"))),
    progress,
    el("div", { class: "stage" },
      el("button", { class: "nav-btn", title: "Previous (←)", "aria-label": "Previous", disabled: s.i === 0, onclick: () => step(-1) }, icon("back", 28)),
      flip,
      el("button", { class: "nav-btn", title: "Next (→)", "aria-label": "Next", onclick: () => step(1) }, icon("forward", 28))),
    el("footer", { class: "study-foot" }, footButtons()),
  );
}

function footButtons() {
  return s.flipped
    ? [
      el("button", { class: "btn big again", onclick: () => answer(false) }, icon("undo", 18), "Again", el("kbd", { text: "1" })),
      el("button", { class: "btn big got", onclick: () => answer(true) }, icon("check", 18), "Got it", el("kbd", { text: "2" })),
    ]
    : [el("button", { class: "btn big", onclick: turn }, icon("flip", 18), "Show", el("kbd", { text: "Space" }))];
}

/** Flip the card (animated: the side changes in the middle); only the buttons below change. */
let turnTimer = 0;
function turn() {
  s.flipped = !s.flipped;
  const f = document.querySelector("#study-view .flip");
  document.querySelector("#study-view .study-foot")?.replaceChildren(...footButtons());
  if (!f) return;
  const flipped = s.flipped;
  clearTimeout(turnTimer);
  f.classList.remove("turning");
  void f.offsetWidth; // restart the animation
  f.classList.add("turning");
  turnTimer = setTimeout(() => f.classList.toggle("flipped", flipped), 160);
  setTimeout(() => f.classList.remove("turning"), 340);
}

function step(by) {
  const i = s.i + by;
  if (i < 0) return;
  s.i = Math.min(i, s.list.length);
  s.flipped = false;
  render();
}

async function answer(known) {
  const id = s.list[s.i];
  (known ? s.got : s.again).add(id);
  (known ? s.again : s.got).delete(id);
  deckCommand("card_review", { id, known });
  step(1);
}

function finished(v) {
  const missed = [...s.again];
  v.replaceChildren(
    el("header", { class: "study-bar" },
      el("div", { class: "grow" }, el("div", { class: "study-title", text: s.title })),
      el("button", { class: "icon-btn close-btn", title: "Close (Esc)", "aria-label": "Close", onclick: close }, icon("close"))),
    el("div", { class: "study-done" },
      el("h2", { text: "Done!" }),
      el("p", { text: `${plural(s.list.length, "card", "cards")}: ${s.got.size} got it, ${s.again.size} to repeat.` }),
      el("div", { class: "row-actions center" },
        missed.length ? el("button", { class: "btn primary", text: `Repeat the ${plural(missed.length, "card", "cards")} I missed`, onclick: () => {
          s.all = missed; s.startAt = null; s.got.clear(); s.again.clear(); arrange(); render();
        } }) : null,
        el("button", { class: "btn", text: "Start over", onclick: () => { s.startAt = null; s.got.clear(); s.again.clear(); arrange(); render(); } }),
        el("button", { class: "btn", text: "Close", onclick: close }))));
  v.querySelector(".study-done .btn")?.focus();
}

// keys on the whole window: clicking an option redraws the view and drops the focus
document.addEventListener("keydown", keys);
function keys(e) {
  if (!s) return;
  if (e.target.matches("input, textarea, select")) return;
  const k = e.key;
  if (k === "Escape") { e.preventDefault(); e.stopPropagation(); close(); return; }
  if (s.i >= s.list.length) return;
  // holding a key repeats it: fine for moving, not for flipping or answering
  if (e.repeat && k !== "ArrowLeft" && k !== "ArrowRight") { e.preventDefault(); return; }
  if (k === " " || k === "Enter" || k === "ArrowUp" || k === "ArrowDown") { e.preventDefault(); turn(); }
  else if (k === "ArrowLeft") { e.preventDefault(); step(-1); }
  else if (k === "ArrowRight") { e.preventDefault(); step(1); }
  else if ((k === "1" || k === "j") && s.flipped) { e.preventDefault(); answer(false); }
  else if ((k === "2" || k === "k") && s.flipped) { e.preventDefault(); answer(true); }
}
