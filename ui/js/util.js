// Small helpers shared by every part of the window.

export const { invoke } = window.__TAURI__.core;
export const { listen } = window.__TAURI__.event;

export const $ = (id) => document.getElementById(id);

/** el("div", {class: "x", onclick: f, text: "hi"}, child, ...) */
export function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on") && typeof v === "function") e.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c != null && c !== false) e.append(c);
  return e;
}

const ICONS = {
  back: "M15 5 8 12l7 7",
  forward: "m9 5 7 7-7 7",
  close: "M6 6l12 12M18 6 6 18",
  up: "M6 15l6-6 6 6",
  down: "M6 9l6 6 6-6",
  chevron: "m9 6 6 6-6 6",
  plus: "M12 5v14M5 12h14",
  folder: "M3.5 7.5A2 2 0 0 1 5.5 5.5h4l2 2h7a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z",
  inbox: "M4 13.5 6.5 6h11l2.5 7.5V18a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18zM4 13.5h4.5l1.5 2h4l1.5-2H20",
  stack: "M7 4.5h10A1.5 1.5 0 0 1 18.5 6v12A1.5 1.5 0 0 1 17 19.5H7A1.5 1.5 0 0 1 5.5 18V6A1.5 1.5 0 0 1 7 4.5zM9 9h6M9 12.5h6M9 16h3.5",
  book: "M4 5.5A1.5 1.5 0 0 1 5.5 4H11v16H5.5A1.5 1.5 0 0 1 4 18.5zM13 4h5.5A1.5 1.5 0 0 1 20 5.5v13a1.5 1.5 0 0 1-1.5 1.5H13z",
  bookmark: "M7 4h10a1 1 0 0 1 1 1v15l-6-4-6 4V5a1 1 0 0 1 1-1z",
  play: "M8 5.5v13l10.5-6.5z",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  gear: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
  shuffle: "M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5",
  check: "M5 12.5 10 17.5 19 7",
  undo: "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
  flip: "M4 12a8 8 0 0 1 14-5.3M20 12a8 8 0 0 1-14 5.3M18 3v4h-4M6 21v-4h4",
  trash: "M4 7h16M9 7V4.5h6V7M6.5 7l1 13h9l1-13",
  edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5 21 21",
};

export function icon(name, size) {
  const ns = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(ns, "svg");
  s.setAttribute("viewBox", "0 0 24 24");
  s.setAttribute("aria-hidden", "true");
  if (size) {
    s.style.width = `${size}px`;
    s.style.height = `${size}px`;
  }
  const p = document.createElementNS(ns, "path");
  p.setAttribute("d", ICONS[name] || name);
  s.append(p);
  return s;
}

export function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}

/** A calm colour per dictionary, from its id. */
export function dictColor(id) {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return `hsl(${h} 55% 55%)`;
}

// Window preferences (theme, text size, study options): kept in this browser profile only.
export const prefs = {
  get(key, fallback) {
    try {
      const v = localStorage.getItem(`kosha.${key}`);
      return v === null ? fallback : JSON.parse(v);
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try { localStorage.setItem(`kosha.${key}`, JSON.stringify(value)); } catch { /* private mode */ }
  },
};

/** A short message at the bottom of the window. */
let toastTimer = 0;
export function toast(text, kind = "") {
  let t = $("toast");
  if (!t) {
    t = el("div", { id: "toast", role: "status" });
    document.body.append(t);
  }
  t.className = `show ${kind}`;
  t.textContent = text;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.className = ""; }, 2600);
}

/** Run an action; show its error instead of failing silently. */
export async function attempt(promise) {
  try {
    return await promise;
  } catch (e) {
    toast(String(e), "error");
    return null;
  }
}

/** A small menu next to `anchor`: items are {label, icon?, onclick, danger?, indent?, checked?} or "-". */
export function menu(anchor, items) {
  closeMenu();
  const m = el("div", { class: "menu", role: "menu" });
  for (const it of items) {
    if (it === "-") {
      m.append(el("div", { class: "menu-sep" }));
      continue;
    }
    if (it.header) {
      m.append(el("div", { class: "menu-header", text: it.header }));
      continue;
    }
    const b = el("button", {
      class: `menu-item${it.danger ? " danger" : ""}${it.checked ? " checked" : ""}`,
      role: "menuitem",
      style: it.indent ? `padding-left: ${12 + it.indent * 16}px` : null,
      onclick: () => { closeMenu(); it.onclick(); },
    }, it.icon ? icon(it.icon, 16) : el("span", { class: "menu-gap" }), el("span", { text: it.label }));
    m.append(b);
  }
  document.body.append(m);
  const r = anchor.getBoundingClientRect();
  const w = m.offsetWidth, h = m.offsetHeight;
  let x = Math.min(r.right - w, window.innerWidth - w - 8);
  if (x < 8) x = Math.min(r.left, window.innerWidth - w - 8);
  let y = r.bottom + 4;
  if (y + h > window.innerHeight - 8) y = Math.max(8, r.top - h - 4);
  m.style.left = `${Math.max(8, x)}px`;
  m.style.top = `${y}px`;
  m.querySelector(".menu-item")?.focus();
  setTimeout(() => document.addEventListener("mousedown", outside, true), 0);
}
function outside(e) {
  if (!e.target.closest(".menu")) closeMenu();
}
export function closeMenu() {
  document.querySelectorAll(".menu").forEach((m) => m.remove());
  document.removeEventListener("mousedown", outside, true);
}
export function menuOpen() {
  return !!document.querySelector(".menu");
}

/** Ask for a line of text in a small dialog; resolves to the text, or null when cancelled. */
export function ask(title, value = "", okLabel = "OK") {
  return new Promise((resolve) => {
    const input = el("input", { type: "text", value, spellcheck: "false" });
    const done = (v) => { back.remove(); resolve(v); };
    const form = el("form", { class: "dialog", onsubmit: (e) => { e.preventDefault(); done(input.value.trim() || null); } },
      el("h3", { text: title }),
      input,
      el("div", { class: "dialog-buttons" },
        el("button", { type: "button", class: "btn", text: "Cancel", onclick: () => done(null) }),
        el("button", { type: "submit", class: "btn primary", text: okLabel })));
    const back = el("div", { class: "dialog-back", onmousedown: (e) => { if (e.target === back) done(null); } }, form);
    form.addEventListener("keydown", (e) => { if (e.key === "Escape") { e.stopPropagation(); done(null); } });
    document.body.append(back);
    input.focus();
    input.select();
  });
}

/** Ask a yes/no question with up to three choices; resolves to the chosen value or null. */
export function choose(title, text, choices) {
  return new Promise((resolve) => {
    const done = (v) => { back.remove(); resolve(v); };
    const box = el("div", { class: "dialog", role: "alertdialog" },
      el("h3", { text: title }),
      text ? el("p", { text }) : null,
      el("div", { class: "dialog-buttons" },
        el("button", { class: "btn", text: "Cancel", onclick: () => done(null) }),
        ...choices.map((c) => el("button", { class: `btn ${c.kind || ""}`, text: c.label, onclick: () => done(c.value) }))));
    const back = el("div", { class: "dialog-back", onmousedown: (e) => { if (e.target === back) done(null); } }, box);
    box.addEventListener("keydown", (e) => { if (e.key === "Escape") { e.stopPropagation(); done(null); } });
    document.body.append(back);
    box.querySelector(".btn:last-child").focus();
  });
}
