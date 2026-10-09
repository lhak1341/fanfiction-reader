// Shared mechanics behind every localStorage-backed preference (architecture
// review candidate 4, 2026-07-23) — the SSR guard and parse-failure fallback
// were hand-rolled five times with only the key/default/(de)serialization
// actually varying. Doesn't own "what counts as a complete value" (e.g.
// merging a parsed object over its own defaults) — that stays in each
// caller's own `deserialize`, since only some prefs want it (an object pref
// wants a field-by-field merge; a Set or a raw string wants pure replace).

export interface PersistedPrefOptions<T> {
  serialize?: (value: T) => string;
  deserialize?: (raw: string) => T;
}

export function definePersistedPref<T>(
  key: string,
  defaultValue: T,
  options: PersistedPrefOptions<T> = {}
) {
  const serialize = options.serialize ?? ((value: T) => JSON.stringify(value));
  const deserialize = options.deserialize ?? ((raw: string) => JSON.parse(raw) as T);

  return {
    get(): T {
      if (typeof localStorage === "undefined") return defaultValue;
      const raw = localStorage.getItem(key);
      if (raw === null) return defaultValue;
      try {
        return deserialize(raw);
      } catch {
        return defaultValue;
      }
    },
    save(value: T) {
      if (typeof localStorage === "undefined") return;
      localStorage.setItem(key, serialize(value));
    },
  };
}
