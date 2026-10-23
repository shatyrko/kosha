// What several parts of the window share: the word-card deck and the dictionaries' state.
// Changes are announced with `on("deck", f)` / `on("library", f)`.

import { invoke, attempt, ask, prefs, toast } from "./util.js";

const bus = new EventTarget();
export const on = (name, f) => bus.addEventListener(name, () => f());
const emit = (name) => bus.dispatchEvent(new Event(name));

export const store = {
  deck: { folders: [], cards: [] },
  deckError: null,
  ready: false,
  dicts: [],          // [{info, enabled}]
  folders: [],        // dictionary folders (paths)
  failed: [],
  loading: true,
};

export async function loadDeck() {
  try {
    store.deck = await invoke("cards");
    store.deckError = null;
  } catch (e) {
    store.deckError = String(e);
  }
  emit("deck");
}

/** Run a card command that returns the new deck. */
export async function deckCommand(name, args) {
  const deck = await attempt(invoke(name, args));
  if (deck) {
    store.deck = deck;
    emit("deck");
  }
  return deck;
}

export async function loadLibrary() {
  const lib = await invoke("library");
  store.loading = lib.loading;
  store.dicts = lib.dicts;
  store.folders = lib.folders;
  store.failed = lib.failed;
  emit("library");
  return lib;
}

export function setReady(ready) {
  store.ready = ready;
}

// ── folders ──────────────────────────────────────────────────
export function folder(id) {
  return store.deck.folders.find((f) => f.id === id) || null;
}

export function children(parent) {
  return store.deck.folders
    .filter((f) => (f.parent || null) === (parent || null))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** Every folder in tree order with its depth: [{folder, depth}]. */
export function tree(parent = null, depth = 0, out = []) {
  for (const f of children(parent)) {
    out.push({ folder: f, depth });
    tree(f.id, depth + 1, out);
  }
  return out;
}

/** The folder and all folders inside it. */
export function subtree(id) {
  const out = [id];
  for (let i = 0; i < out.length; i++) {
    for (const f of store.deck.folders) if (f.parent === out[i]) out.push(f.id);
  }
  return out;
}

export function folderPath(id) {
  const names = [];
  for (let f = folder(id); f; f = f.parent ? folder(f.parent) : null) names.unshift(f.name);
  return names.join(" › ");
}

/** Cards of a view: "all", "unsorted", or a folder id (with its sub-folders when `deep`). */
export function cardsIn(view, deep = true) {
  const cards = store.deck.cards;
  if (view === "all") return cards;
  if (view === "unsorted") return cards.filter((c) => !c.folder);
  const ids = new Set(deep ? subtree(view) : [view]);
  return cards.filter((c) => c.folder && ids.has(c.folder));
}

export function savedCards(word, dictId) {
  const w = word.toLowerCase();
  return store.deck.cards.filter((c) => c.dict_id === dictId && c.word.toLowerCase() === w);
}

export async function newFolder(parent = null) {
  const name = await ask(parent ? `New folder in “${folder(parent)?.name}”` : "New folder", "", "Create");
  if (!name) return null;
  const before = new Set(store.deck.folders.map((f) => f.id));
  const deck = await deckCommand("folder_add", { name, parent });
  return deck ? deck.folders.find((f) => !before.has(f.id)) || null : null;
}

/** Save a word of one dictionary as a card in `folderId` (null: Unsorted). */
export async function saveCard(word, dictId, folderId) {
  const deck = await deckCommand("card_add", { word, dictId, folder: folderId });
  if (deck) {
    prefs.set("lastFolder", folderId);
    toast(`Saved to ${folderId ? folderPath(folderId) : "Unsorted"}`);
  }
  return deck;
}
