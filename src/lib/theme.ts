// theme was the sixth hand-rolled SSR-guard/parse-fallback copy
// persistedPref.ts (architecture review candidate 4) didn't reach yet —
// architecture review candidate 6, 2026-07-24. Raw (unquoted) serialize
// keeps the storage format identical to the old hand-rolled version so an
// existing user's stored value round-trips instead of silently reverting to
// "system" the first time this ships. applyTheme stays DOM-only (no
// localStorage write) so it's callable independently of persistence —
// ThemeToggle's onMount now calls both getTheme() and applyTheme() since
// the pre-refactor version only ever restored the toggle's highlighted
// button, never actually re-applied a stored non-system theme on boot.
import { definePersistedPref } from "./persistedPref";

export type ThemePref = "light" | "dark" | "system";

const pref = definePersistedPref<ThemePref>("ffn-reader-theme", "system", {
  serialize: (v) => v,
  deserialize: (raw) => {
    if (raw !== "light" && raw !== "dark" && raw !== "system")
      throw new Error(`invalid theme: ${raw}`);
    return raw;
  },
});

export const getTheme = pref.get;
export const saveTheme = pref.save;

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
}
