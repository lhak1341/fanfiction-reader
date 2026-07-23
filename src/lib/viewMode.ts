import { definePersistedPref } from "./persistedPref";

export type ViewMode = "list" | "card";

// Stored as the raw string itself (no JSON quoting) — kept that way via
// identity serialize/deserialize so an existing user's on-disk value stays
// readable after this moved onto definePersistedPref.
const pref = definePersistedPref<ViewMode>("ffn-view-mode", "list", {
  serialize: (mode) => mode,
  deserialize: (raw) => (raw === "card" ? "card" : "list")
});

export const getViewMode = pref.get;
export const saveViewMode = pref.save;
