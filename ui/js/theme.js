// Runs in <head>, before anything is drawn: the theme and the text size, so the window never
// shows the light page for a moment before switching to dark (settings.js keeps them current).
(function () {
  var get = function (k, d) {
    try { var v = localStorage.getItem("kosha." + k); return v === null ? d : JSON.parse(v); } catch (e) { return d; }
  };
  var theme = get("theme", "system");
  if (theme === "system") theme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  var size = Math.min(28, Math.max(12, Number(get("size", 17)) || 17));
  document.documentElement.style.setProperty("--article-size", size + "px");
})();
