export type ThemePref = "light" | "dark" | "system";

const STORAGE_KEY = "ffn-reader-theme";

export function getStoredTheme(): ThemePref {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

/// "system" removes the override entirely, letting `@media (prefers-color-scheme)`
/// in app.css drive it. "light"/"dark" set `data-theme`, which app.css's
/// `:root[data-theme=...]` rules override the media query with in both directions.
export function applyTheme(pref: ThemePref) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (pref === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", pref);
  }
  localStorage.setItem(STORAGE_KEY, pref);
}
