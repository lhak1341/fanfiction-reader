// theme was the sixth hand-rolled SSR-guard/parse-fallback copy
// persistedPref.ts (architecture review candidate 4) didn't reach yet —
// architecture review candidate 6, 2026-07-24. Raw (unquoted) serialize
// keeps the storage format identical to the old hand-rolled version so an
// existing user's stored value round-trips instead of silently reverting to
// "system" the first time this ships. Since the React rewrite the kit
// applies the mode; main.tsx reads this pref once to seed KitRoot.
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
