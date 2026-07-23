import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { definePersistedPref } from "./persistedPref";

// This suite runs in a plain Node environment with no --localstorage-file,
// so the real global `localStorage` is genuinely `typeof === "undefined"`
// here — exercising the actual read/write mechanics needs a stand-in.
function makeFakeLocalStorage() {
  const store = new Map<string, string>();
  return {
    getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
    clear: () => store.clear(),
    key: (i: number) => [...store.keys()][i] ?? null,
    get length() {
      return store.size;
    }
  };
}

describe("definePersistedPref with a stubbed localStorage", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", makeFakeLocalStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("round-trips a value through the default JSON serialize/deserialize", () => {
    const pref = definePersistedPref<{ a: number }>("k", { a: 0 });
    pref.save({ a: 5 });
    expect(pref.get()).toEqual({ a: 5 });
  });

  it("falls back to defaultValue when nothing is stored", () => {
    const pref = definePersistedPref<{ a: number }>("missing-key", { a: 1 });
    expect(pref.get()).toEqual({ a: 1 });
  });

  it("falls back to defaultValue when the stored value is corrupt JSON", () => {
    localStorage.setItem("bad-key", "{not json");
    const pref = definePersistedPref<{ a: number }>("bad-key", { a: 2 });
    expect(pref.get()).toEqual({ a: 2 });
  });

  it("uses a custom serialize/deserialize pair instead of JSON when given", () => {
    const pref = definePersistedPref<string>("raw-key", "default", {
      serialize: (v) => v,
      deserialize: (raw) => raw
    });
    pref.save("hello");
    expect(localStorage.getItem("raw-key")).toBe("hello"); // not JSON-quoted
    expect(pref.get()).toBe("hello");
  });
});

describe("definePersistedPref without localStorage available", () => {
  it("get() returns defaultValue", () => {
    const pref = definePersistedPref<{ a: number }>("k", { a: 9 });
    expect(pref.get()).toEqual({ a: 9 });
  });

  it("save() is a no-op rather than throwing", () => {
    const pref = definePersistedPref<{ a: number }>("k", { a: 9 });
    expect(() => pref.save({ a: 1 })).not.toThrow();
  });
});
