// The Sanskrit tab: a verse analysed. The text, its meter and its words are computed from your
// dictionaries (no AI); the explanation after them is written by Google Gemini (your key).

import { $, el, icon, invoke, prefs, toast } from "./util.js";

const input = $("sa-text");
const out = $("sa-result");
const recentEl = $("sa-recent");
let lookUp = () => {};
let openSettings = () => {};
let token = 0;

const EXAMPLE = "श्रीभगवानुवाच ।\nकुतस्त्वा कश्मलमिदं विषमे समुपस्थितम् ।\nअनार्यजुष्टमस्वर्ग्यमकीर्तिकरमर्जुन ॥ २ ॥";

export function init(opts) {
  lookUp = opts.lookUp;
  openSettings = opts.openSettings;
  $("sa-go").onclick = () => analyse();
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); analyse(); }
  });
  $("sa-example").onclick = () => { input.value = EXAMPLE; analyse(); };
  renderRecent();
}

export function focus() {
  input.focus();
}

// ── recent verses (this computer only) ───────────────────────
function recent() {
  return prefs.get("saRecent", []);
}

function remember(text) {
  const list = recent().filter((r) => r.text !== text);
  list.unshift({ text, at: Date.now() });
  prefs.set("saRecent", list.slice(0, 40));
  renderRecent();
}

/** A verse's name in the list: its first line that is not a speaker line ("… uvāca"). */
function label(text) {
  const lines = text.split(/\n|[।|॥]/).map((l) => l.trim()).filter((l) => l && !/^[\d०-९\s]+$/.test(l));
  // "…uvāca" is a speaker line; in Devanāgarī its u merges with the letter before (भगवानुवाच)
  return lines.find((l) => !/(वाच|uvāca|uvaca)$/.test(l)) || lines[0] || text;
}

function renderRecent() {
  const list = recent();
  recentEl.replaceChildren(
    ...(list.length ? [el("li", { class: "group", text: "Recent verses" })] : []),
    ...list.map((r) => el("li", {
      class: "verse-row",
      title: r.text,
      onclick: () => { input.value = r.text; analyse(); },
    }, el("span", { class: "w", text: label(r.text) }))));
}

// ── the analysis ─────────────────────────────────────────────
async function analyse() {
  const text = input.value.trim();
  if (!text) { input.focus(); return; }
  const t = ++token;
  out.replaceChildren(el("div", { class: "online-wait" }, el("span", { class: "spinner" })));
  let a;
  try {
    a = await invoke("sanskrit_analyze", { text });
  } catch (e) {
    if (t === token) out.replaceChildren(el("p", { class: "err", text: String(e) }));
    return;
  }
  if (t !== token) return;
  remember(text);
  const ai = card("Explanation", "AI · Gemini", el("div", { class: "online-wait" }, el("span", { class: "spinner" }), el("span", { class: "sub", text: " Asking Gemini…" })));
  out.replaceChildren(textCard(a), meterCard(a), wordsCard(a), ai);
  explain(a, ai, t);
}

function card(title, tag, ...body) {
  return el("section", { class: "article sa-card" },
    el("div", { class: "article-head" }, el("div", { class: "dict", text: title }), tag ? el("span", { class: "sa-tag", text: tag }) : null),
    ...body);
}

function textCard(a) {
  const lines = a.deva.map((d, i) => el("div", { class: "sa-line" },
    el("div", { class: "sa-deva", text: d }),
    el("div", { class: "sa-iast", text: a.iast[i] })));
  return card("Text", `input: ${a.scheme}`, ...lines);
}

function meterCard(a) {
  const m = a.meter;
  if (!m) return card("Meter", "computed", el("p", { class: "online-none", text: "No verse lines to scan." }));
  const rows = m.padas.map((p, i) => el("div", { class: "sa-pada" },
    el("span", { class: "sa-pada-n", text: String(i + 1) }),
    ...p.map((s) => el("span", { class: `syl ${s.heavy ? "heavy" : "light"}`, title: s.heavy ? "heavy (guru)" : "light (laghu)" },
      el("span", { class: "mark", text: s.heavy ? "—" : "◡" }),
      el("span", { class: "t", text: s.text })))));
  return card("Meter", "computed",
    el("div", { class: "sa-meter-name", text: m.name }),
    el("p", { class: "sub", text: m.note }),
    ...rows);
}

function wordsCard(a) {
  if (!a.words.length) {
    return card("Words", "your dictionaries", el("p", { class: "online-none", text: "No Sanskrit dictionaries are open: add Monier-Williams or Apte in Settings → Dictionaries." }));
  }
  const how = { "as written": "", "dictionary form": "dictionary form", split: "split" };
  const rows = a.words.map((w) => {
    const forms = w.parts.length
      ? w.parts.map((p, i) => [i ? el("span", { class: "plus", text: "+" }) : null,
        el("button", { class: "chip form", title: `${p.deva}: open in the Dictionary`, onclick: () => lookUp(p.deva) }, p.iast)]).flat()
      : [el("span", { class: "online-none", text: "not found" })];
    // per part: its first two entries, the rest behind "more"
    const glosses = w.parts.map((p) => {
      if (!p.meanings.length) return null;
      const line = (m) => el("div", { class: "gloss" }, el("b", { text: `${p.iast}: ` }), m.gloss, el("span", { class: "gloss-dict", text: ` — ${m.dict_name}` }));
      const box = el("div", {}, ...p.meanings.slice(0, 2).map(line));
      const rest = p.meanings.slice(2);
      if (rest.length) {
        const more = el("button", { class: "link-btn more", text: `${rest.length} more…`, onclick: () => { more.replaceWith(...rest.map(line)); } });
        box.append(more);
      }
      return box;
    });
    return el("div", { class: "sa-word" },
      el("div", { class: "sa-word-head" },
        el("span", { class: "sa-word-w", text: w.word }),
        el("span", { class: "sa-word-d", text: w.deva }),
        how[w.how] ? el("span", { class: "sa-how", text: how[w.how] }) : null),
      el("div", { class: "sa-forms" }, ...forms),
      ...glosses);
  });
  return card("Words", "your dictionaries",
    el("p", { class: "sub", text: "Split and looked up in your Sanskrit dictionaries. “Dictionary form” and “split” are found by rules (endings, joined words), not by a full grammar: check them against the entries." }),
    ...rows);
}

// ── Gemini ───────────────────────────────────────────────────
async function explain(a, box, t) {
  let r;
  try {
    r = await invoke("sanskrit_explain", { analysis: a });
  } catch (e) {
    if (t !== token) return;
    const body = String(e) === "no-key"
      ? [el("p", { text: "Gemini can add a translation, the commentaries' readings and how to use the verse. It needs a free key from Google AI Studio." }),
        el("button", { class: "btn primary", text: "Add a Gemini key…", onclick: () => openSettings("online") })]
      : [el("p", { class: "err", text: String(e) })];
    box.replaceChildren(box.firstChild, ...body);
    return;
  }
  if (t !== token) return;
  const s = (v) => (typeof v === "string" ? v.trim() : "");
  const parts = [el("p", { class: "sa-ai-note", text: "Written by AI (Gemini): it can be wrong. The text, meter and words above are computed from your dictionaries." })];
  if (s(r.source)) parts.push(el("div", { class: "sa-source" }, icon("book", 15), el("span", { text: r.source })));
  if (Array.isArray(r.literal) && r.literal.length) {
    parts.push(el("h3", { text: "Literal, line by line" }), el("ol", { class: "sa-literal" }, ...r.literal.map((l) => el("li", { text: s(l) }))));
  }
  if (s(r.translation)) parts.push(el("h3", { text: "Translation" }), el("p", { class: "translation", text: r.translation }));
  if (Array.isArray(r.words) && r.words.length) {
    parts.push(el("h3", { text: "Words in this verse" }), el("dl", { class: "sa-senses" },
      ...r.words.flatMap((w) => [el("dt", { text: s(w.word) }), el("dd", { text: s(w.sense) })])));
  }
  if (Array.isArray(r.interpretation) && r.interpretation.length) {
    parts.push(el("h3", { text: "Interpretation" }));
    for (const i of r.interpretation) parts.push(el("div", { class: "sa-school" }, el("b", { text: s(i.school) }), el("p", { text: s(i.text) })));
  }
  if (s(r.application)) parts.push(el("h3", { text: "Application" }), el("p", { text: r.application }));
  if (s(r.notes)) parts.push(el("h3", { text: "Notes" }), el("p", { class: "sub", text: r.notes }));
  box.replaceChildren(box.firstChild, ...parts);
}

export function testGemini() {
  return invoke("sanskrit_analyze", { text: "सत्यमेव जयते" }).then((a) => invoke("sanskrit_explain", { analysis: a }));
}

export function toastError(e) {
  toast(String(e), "error");
}
