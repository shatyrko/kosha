// Word cards: the folder tree in the sidebar, the cards of a folder, one card in detail.

import { $, el, icon, plural, prefs, menu, choose, ask, toast, dictColor } from "./util.js";
import { store, on, tree, children, folder, folderPath, cardsIn, newFolder, deckCommand, subtree } from "./store.js";
import * as study from "./study.js";
import { play } from "./dictionary.js";

const treeEl = $("folder-tree");
const grid = $("card-grid");
const filter = $("card-filter");

let view = prefs.get("cardsView", "all"); // "all", "unsorted" or a folder id
let collapsed = new Set(prefs.get("collapsed", []));
let lookUp = () => {};
let detail = null; // the open card dialog: {list, index}

const DRAG = "application/x-kosha";

export function init(opts) {
  lookUp = opts.lookUp;
  on("deck", render);
  filter.addEventListener("input", renderGrid);
  $("new-folder").onclick = async () => {
    const f = await newFolder(isFolder(view) ? null : null);
    if (f) select(f.id);
  };
  $("study").onclick = () => startStudy();
  $("folder-more").onclick = (e) => folderMenu(e.currentTarget, isFolder(view) ? view : null);
  $("deep").checked = prefs.get("deep", true);
  $("deep").onchange = () => { prefs.set("deep", $("deep").checked); renderGrid(); };
}

const isFolder = (v) => v !== "all" && v !== "unsorted";

export function select(v) {
  view = v;
  prefs.set("cardsView", v);
  render();
}

function viewName(v) {
  if (v === "all") return "All cards";
  if (v === "unsorted") return "Unsorted";
  return folder(v)?.name || "All cards";
}

/** The cards on screen: the view, the sub-folders switch, the filter. */
function visibleCards() {
  let list = cardsIn(view, $("deep").checked);
  const f = filter.value.trim().toLowerCase();
  if (f) {
    list = list.filter((c) =>
      c.word.toLowerCase().includes(f) || c.text.toLowerCase().includes(f) || c.note.toLowerCase().includes(f));
  }
  return [...list].sort((a, b) => b.added - a.added);
}

export function render() {
  if (isFolder(view) && !folder(view)) view = "all";
  renderTree();
  renderGrid();
}

// ── the tree ─────────────────────────────────────────────────
function renderTree() {
  const rows = [];
  const count = (v) => cardsIn(v, true).length;
  rows.push(row({ v: "all", name: "All cards", iconName: "stack", n: store.deck.cards.length }));
  rows.push(row({ v: "unsorted", name: "Unsorted", iconName: "inbox", n: count("unsorted") }));
  if (store.deck.folders.length) rows.push(el("li", { class: "group", text: "Folders" }));
  const walk = (parent, depth) => {
    for (const f of children(parent)) {
      const kids = children(f.id).length > 0;
      rows.push(row({ v: f.id, name: f.name, iconName: "folder", n: count(f.id), depth, kids, folderId: f.id }));
      if (kids && !collapsed.has(f.id)) walk(f.id, depth + 1);
    }
  };
  walk(null, 0);
  treeEl.replaceChildren(...rows);
}

function row({ v, name, iconName, n, depth = 0, kids = false, folderId = null }) {
  const li = el("li", {
    class: `tree-row${v === view ? " sel" : ""}`,
    style: `--depth: ${depth}`,
    tabindex: "0",
    onclick: () => select(v),
    onkeydown: (e) => { if (e.key === "Enter") select(v); },
  });
  const twisty = el("button", {
    class: `twisty${kids ? "" : " none"}${collapsed.has(v) ? "" : " open"}`,
    "aria-label": collapsed.has(v) ? "Open" : "Close",
    tabindex: "-1",
    onclick: (e) => {
      e.stopPropagation();
      if (!kids) return;
      if (collapsed.has(v)) collapsed.delete(v); else collapsed.add(v);
      prefs.set("collapsed", [...collapsed]);
      renderTree();
    },
  }, icon("chevron", 14));
  li.append(twisty, icon(iconName, 17), el("span", { class: "name", text: name }), el("span", { class: "count", text: n ? String(n) : "" }));
  if (folderId) {
    li.append(el("button", {
      class: "icon-btn row-more",
      title: "Folder actions",
      "aria-label": "Folder actions",
      onclick: (e) => { e.stopPropagation(); folderMenu(e.currentTarget, folderId); },
    }, icon("more", 16)));
    // folders can be dragged into other folders
    li.draggable = true;
    li.addEventListener("dragstart", (e) => {
      e.dataTransfer.setData(DRAG, JSON.stringify({ folder: folderId }));
      e.dataTransfer.effectAllowed = "move";
    });
  }
  if (v !== "all") {
    li.addEventListener("dragover", (e) => {
      if (![...e.dataTransfer.types].includes(DRAG)) return;
      e.preventDefault();
      li.classList.add("drop");
    });
    li.addEventListener("dragleave", () => li.classList.remove("drop"));
    li.addEventListener("drop", (e) => {
      e.preventDefault();
      li.classList.remove("drop");
      let data;
      try { data = JSON.parse(e.dataTransfer.getData(DRAG)); } catch { return; }
      const target = v === "unsorted" ? null : v;
      if (data.card) moveCard(data.card, target);
      else if (data.folder && data.folder !== target) {
        deckCommand("folder_move", { id: data.folder, parent: target }).then((d) =>
          d && toast(target ? `Moved into ${folderPath(target)}` : "Moved to the top level"));
      }
    });
  }
  return li;
}

async function folderMenu(anchor, id) {
  const items = [{ label: id ? "New folder inside" : "New folder", icon: "plus", onclick: async () => {
    const f = await newFolder(id);
    if (f) {
      if (id) { collapsed.delete(id); prefs.set("collapsed", [...collapsed]); }
      select(f.id);
    }
  } }];
  if (id) {
    items.push({ label: "Rename…", icon: "edit", onclick: () => renameFolder(id) });
    if (folder(id)?.parent) items.push({ label: "Move to the top level", icon: "folder", onclick: () => deckCommand("folder_move", { id, parent: null }) });
    items.push("-");
    items.push({ label: "Delete folder…", icon: "trash", danger: true, onclick: () => deleteFolder(id) });
  }
  menu(anchor, items);
}

async function renameFolder(id) {
  const name = await ask("Rename folder", folder(id)?.name || "", "Rename");
  if (name) deckCommand("folder_rename", { id, name });
}

async function deleteFolder(id) {
  const f = folder(id);
  const inside = cardsIn(id, true).length;
  const subs = subtree(id).length - 1;
  if (!inside && !subs) {
    await deckCommand("folder_delete", { id, keepContents: true });
    return;
  }
  const what = [inside ? plural(inside, "card", "cards") : null, subs ? plural(subs, "folder", "folders") : null].filter(Boolean).join(" and ");
  const up = f.parent ? `“${folder(f.parent)?.name}”` : "Unsorted";
  const choice = await choose(`Delete “${f.name}”?`, `It has ${what}.`, [
    { label: `Keep the cards (to ${up})`, value: "keep" },
    { label: "Delete everything", value: "all", kind: "danger" },
  ]);
  if (!choice) return;
  const d = await deckCommand("folder_delete", { id, keepContents: choice === "keep" });
  if (d) toast(`Deleted “${f.name}”`);
}

function moveCard(id, target) {
  deckCommand("card_update", { id, patch: { folder: target } }).then((d) =>
    d && toast(`Moved to ${target ? folderPath(target) : "Unsorted"}`));
}

// ── the cards of the view ────────────────────────────────────
function renderGrid() {
  const list = visibleCards();
  $("cards-title").textContent = isFolder(view) ? folderPath(view) : viewName(view);
  const known = list.filter((c) => c.known).length;
  $("cards-count").textContent = list.length
    ? `${plural(list.length, "card", "cards")}${known ? ` · ${known} known` : ""}`
    : "";
  $("study").disabled = list.length === 0;
  $("deep-label").hidden = !isFolder(view) || subtree(view).length < 2;
  $("folder-more").hidden = view === "all" || view === "unsorted";
  if (store.deckError) {
    grid.replaceChildren(el("div", { class: "empty" }, el("h2", { text: "Your cards can't be read" }), el("p", { text: store.deckError })));
    return;
  }
  if (!list.length) {
    const none = store.deck.cards.length === 0;
    grid.replaceChildren(el("div", { class: "empty" },
      icon("bookmark", 44),
      el("h2", { text: none ? "No word cards yet" : filter.value ? "No cards match" : "This folder is empty" }),
      el("p", { text: none
        ? "In the dictionary, press Save on an article (or Ctrl+S) to keep the word here and learn it later."
        : "Drag cards here from another folder, or save words into it from the dictionary." })));
    return;
  }
  grid.replaceChildren(...list.map((c, i) => tile(c, i, list)));
}

/** The start of the article, without the word itself where the article begins with it. */
function preview(c) {
  let lines = c.text.split("\n").map((l) => l.trim()).filter(Boolean);
  const w = c.word.toLowerCase();
  if (lines[0] && lines[0].toLowerCase().startsWith(w)) {
    lines[0] = lines[0].slice(w.length).trim();
    if (!lines[0]) lines = lines.slice(1);
  }
  return lines.slice(0, 3).join(" · ");
}

function tile(c, i, list) {
  const t = el("div", {
    class: `tile${c.known ? " known" : ""}`,
    tabindex: "0",
    draggable: "true",
    style: `--dot: ${dictColor(c.dict_id)}`,
    onclick: () => openCard(list, i),
    onkeydown: (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openCard(list, i); } },
  },
  el("div", { class: "tile-word", text: c.word }),
  c.headword && c.headword.toLowerCase() !== c.word.toLowerCase() ? el("div", { class: "tile-hw", text: c.headword }) : null,
  el("div", { class: "tile-text", text: c.note || preview(c) }),
  el("div", { class: "tile-meta" },
    el("span", { class: "tile-dict", text: c.dict_name }),
    view === "all" || ($("deep").checked && c.folder !== view)
      ? el("span", { class: "tile-folder", text: c.folder ? folder(c.folder)?.name || "" : "" })
      : null,
    c.known ? el("span", { class: "tile-known", title: "Marked “Got it” in study" }, icon("check", 14)) : null));
  t.addEventListener("dragstart", (e) => {
    e.dataTransfer.setData(DRAG, JSON.stringify({ card: c.id }));
    e.dataTransfer.effectAllowed = "move";
  });
  return t;
}

// ── one card ─────────────────────────────────────────────────
function openCard(list, index) {
  detail = { list, index };
  renderDetail();
}

function folderSelect(card) {
  const s = el("select", { "aria-label": "Folder" });
  s.append(el("option", { value: "", text: "Unsorted" }));
  for (const { folder: f, depth } of tree()) {
    s.append(el("option", { value: f.id, text: `${"   ".repeat(depth)}${f.name}` }));
  }
  s.value = card.folder || "";
  s.onchange = () => moveCard(card.id, s.value || null);
  return s;
}

function renderDetail() {
  $("card-dialog")?.remove();
  if (!detail) return;
  const c = store.deck.cards.find((x) => x.id === detail.list[detail.index]?.id);
  if (!c) { detail = null; return; }
  const note = el("textarea", { placeholder: "Your note: an example, a mnemonic, the context where you met the word…", rows: "3" });
  note.value = c.note;
  let saved = c.note;
  const saveNote = () => {
    if (note.value !== saved) {
      saved = note.value;
      deckCommand("card_update", { id: c.id, patch: { note: note.value } });
    }
  };
  note.addEventListener("blur", saveNote);
  // also while typing, after a pause, so nothing typed is lost
  let noteTimer = 0;
  note.addEventListener("input", () => { clearTimeout(noteTimer); noteTimer = setTimeout(saveNote, 700); });
  const body = el("div", { class: "card-article" });
  body.innerHTML = c.html; // a copy of the engine's article HTML
  body.addEventListener("click", (e) => {
    const sound = e.target.closest("button.sound");
    if (sound) { play(sound); return; }
    const ref = e.target.closest("a.ref");
    if (ref) { e.preventDefault(); saveNote(); close(); lookUp(ref.dataset.word); return; }
    if (e.target.closest("a")) e.preventDefault();
  });
  const n = detail.list.length;
  const dlg = el("div", { id: "card-dialog", class: "dialog-back", onmousedown: (e) => { if (e.target.id === "card-dialog") close(); } },
    el("div", { class: "card-detail", role: "dialog", "aria-label": c.word },
      el("header", {},
        el("button", { class: "icon-btn", title: "Previous (←)", disabled: detail.index === 0, onclick: () => step(-1) }, icon("back")),
        el("div", { class: "grow" },
          el("h2", { text: c.word }),
          el("div", { class: "sub", text: `${c.dict_name} · saved ${new Date(c.added * 1000).toLocaleDateString()}${c.reviews ? ` · studied ${plural(c.reviews, "time", "times")}` : ""}` })),
        el("span", { class: "pos", text: `${detail.index + 1} / ${n}` }),
        el("button", { class: "icon-btn", title: "Next (→)", disabled: detail.index >= n - 1, onclick: () => step(1) }, icon("forward")),
        el("button", { class: "icon-btn", title: "Close (Esc)", onclick: close }, icon("close"))),
      body,
      el("label", { class: "field" }, el("span", { text: "Note" }), note),
      el("div", { class: "card-actions" },
        el("label", { class: "field inline" }, el("span", { text: "Folder" }), folderSelect(c)),
        el("label", { class: "check" },
          (() => {
            const k = el("input", { type: "checkbox" });
            k.checked = c.known;
            k.onchange = () => deckCommand("card_update", { id: c.id, patch: { known: k.checked } });
            return k;
          })(),
          "I know it"),
        el("div", { class: "grow" }),
        el("button", { class: "btn", text: "Look up", onclick: () => { saveNote(); close(); lookUp(c.word); } }),
        el("button", { class: "btn", text: "Study from here", onclick: () => { saveNote(); const l = detail.list; const i = detail.index; close(); study.start(l, viewName(view), i); } }),
        el("button", { class: "btn danger", text: "Delete", onclick: async () => {
          const ok = await choose(`Delete the card “${c.word}”?`, "", [{ label: "Delete", value: true, kind: "danger" }]);
          if (ok) { await deckCommand("card_delete", { id: c.id }); close(); }
        } }))));
  dlg.addEventListener("keydown", (e) => {
    if (e.target === note) {
      if (e.key === "Escape") { e.stopPropagation(); saveNote(); dlg.querySelector(".card-detail").focus(); }
      return;
    }
    if (e.key === "Escape") { e.stopPropagation(); close(); }
    else if (e.key === "ArrowLeft") { e.preventDefault(); step(-1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); step(1); }
  });
  document.body.append(dlg);
  dlg.querySelector(".card-detail").tabIndex = -1;
  dlg.querySelector(".card-detail").focus();

  function step(by) {
    saveNote();
    const i = detail.index + by;
    if (i < 0 || i >= detail.list.length) return;
    detail.index = i;
    renderDetail();
  }
  function close() {
    saveNote();
    detail = null;
    $("card-dialog")?.remove();
  }
}

// refresh an open card when the deck changes (e.g. moved to another folder)
on("deck", () => { if (detail && $("card-dialog") && !document.activeElement?.matches("textarea")) renderDetail(); });

export function startStudy() {
  const list = visibleCards();
  if (list.length) study.start(list, isFolder(view) ? folderPath(view) : viewName(view), 0);
}

export function dialogOpen() {
  return !!$("card-dialog");
}
