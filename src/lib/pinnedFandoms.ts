import { definePersistedPref } from "./persistedPref";

const pref = definePersistedPref<Set<string>>("ffn-pinned-fandoms", new Set(), {
  serialize: (fandoms) => JSON.stringify([...fandoms]),
  deserialize: (raw) => new Set(JSON.parse(raw))
});

export const getPinnedFandoms = pref.get;
export const savePinnedFandoms = pref.save;
