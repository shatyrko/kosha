// Dragging inside the window, done by the page itself (as in Charon) instead of the webview's
// native drag and drop, which draws what it drags two or three times too large on a scaled
// display and cannot be cancelled cleanly. A press that moves 6 px becomes a drag; a small pill
// with the name follows the pointer; the caller says on every move what a release would do
// (and marks it); Escape or a lost pointer puts it down; lists scroll at their edges; the click
// a release would make is swallowed.

import { el, icon } from "./util.js";

const THRESHOLD = 6;
let active = null; // the drag going on

/**
 * Make `handle` start drags.
 *   label():          text of the pill
 *   iconName:         icon of the pill
 *   over(x, y, el):   called on every move with the element under the pointer; returns a
 *                     target (anything) or null, and marks it
 *   drop(target):     the release
 *   clear():          remove the marks
 *   start(), finish(): when the drag begins and ends (e.g. dim what is dragged)
 */
export function draggable(handle, { label, iconName = "grip", over, drop, clear = () => {}, start: onStart = () => {}, finish = () => {} }) {
  handle.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || active) return;
    // no text selection from a press on something draggable (clicks still work)
    if (!e.target.closest("input, textarea, select")) e.preventDefault();
    const start = { x: e.clientX, y: e.clientY };
    const move = (ev) => {
      if (!active) {
        if (Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < THRESHOLD) return;
        begin(ev);
      }
      update(ev);
    };
    const up = (ev) => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", cancel);
      if (!active) return;
      update(ev);
      const target = active.target;
      end();
      // the release must not also click what is under it
      window.addEventListener("click", swallow, true);
      setTimeout(() => window.removeEventListener("click", swallow, true), 0);
      if (target != null) drop(target);
    };
    const cancel = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", cancel);
      if (active) end();
    };
    const begin = (ev) => {
      handle.setPointerCapture(ev.pointerId);
      // the press may have started selecting text: a drag never selects
      window.getSelection()?.removeAllRanges();
      const ghost = el("div", { class: "drag-ghost" }, icon(iconName, 15), el("span", { text: label() }));
      document.body.append(ghost);
      document.body.classList.add("dragging-something");
      active = { ghost, target: null, clear, finish, scrollTimer: 0 };
      onStart();
    };
    const update = (ev) => {
      active.ghost.style.transform = `translate(${ev.clientX + 14}px, ${ev.clientY + 10}px)`;
      const under = document.elementFromPoint(ev.clientX, ev.clientY);
      clear();
      active.target = under ? over(ev.clientX, ev.clientY, under) : null;
      active.ghost.classList.toggle("no-drop", active.target == null);
      edgeScroll(under, ev);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", cancel);
  });
}

function swallow(e) {
  e.stopPropagation();
  e.preventDefault();
  window.removeEventListener("click", swallow, true);
}

function end() {
  if (!active) return;
  clearInterval(active.scrollTimer);
  active.clear();
  active.finish();
  active.ghost.remove();
  document.body.classList.remove("dragging-something");
  active = null;
}

/** Scroll the list under the pointer while it is near its top or bottom edge. */
function edgeScroll(under, ev) {
  clearInterval(active.scrollTimer);
  let s = under;
  while (s && s !== document.body) {
    const style = getComputedStyle(s);
    if (/(auto|scroll)/.test(style.overflowY) && s.scrollHeight > s.clientHeight) break;
    s = s.parentElement;
  }
  if (!s || s === document.body) return;
  const r = s.getBoundingClientRect();
  const zone = 36;
  const by = ev.clientY < r.top + zone ? -12 : ev.clientY > r.bottom - zone ? 12 : 0;
  if (by) active.scrollTimer = setInterval(() => { s.scrollTop += by; }, 30);
}

// Escape puts a drag down
document.addEventListener("keydown", (e) => {
  if (active && e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    end();
  }
}, true);
window.addEventListener("blur", () => end());

export function dragging() {
  return !!active;
}
