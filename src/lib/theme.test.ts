import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getTheme, saveTheme } from "./theme";

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
    },
  };
}

describe("theme pref", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", makeFakeLocalStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("round-trips a valid theme as a raw (unquoted) string", () => {
    saveTheme("dark");
    expect(localStorage.getItem("ffn-reader-theme")).toBe("dark");
    expect(getTheme()).toBe("dark");
  });

  it("falls back to system when nothing is stored", () => {
    expect(getTheme()).toBe("system");
  });

  it("falls back to system when the stored value isn't a valid ThemePref", () => {
    localStorage.setItem("ffn-reader-theme", "blue");
    expect(getTheme()).toBe("system");
  });
});
