import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { applyFandomRename, getAo3Tag, renameAo3Tag, saveAo3Tag } from "./fandom";

// Plain Node environment: the real global `localStorage` is undefined, so the
// tag map needs a stand-in to be exercised (same approach as persistedPref.test.ts).
function stubLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
    setItem: (key: string, value: string) => {
      store.set(key, value);
    }
  });
}

describe("renameAo3Tag", () => {
  beforeEach(stubLocalStorage);
  afterEach(() => vi.unstubAllGlobals());

  it("moves the tag to the new fandom name", () => {
    saveAo3Tag("Naruto", "Naruto (Anime & Manga)");
    renameAo3Tag("Naruto", "Naruto (Anime)");
    expect(getAo3Tag("Naruto (Anime)")).toBe("Naruto (Anime & Manga)");
    expect(getAo3Tag("Naruto")).toBeUndefined();
  });

  it("keeps the target's own tag when merging into a fandom that already has one", () => {
    saveAo3Tag("Naruto", "old tag");
    saveAo3Tag("Naruto (Anime)", "kept tag");
    renameAo3Tag("Naruto", "Naruto (Anime)");
    expect(getAo3Tag("Naruto (Anime)")).toBe("kept tag");
    expect(getAo3Tag("Naruto")).toBeUndefined();
  });

  it("leaves the map alone when the old fandom has no tag", () => {
    saveAo3Tag("Bleach", "Bleach tag");
    renameAo3Tag("Naruto", "Naruto (Anime)");
    expect(getAo3Tag("Bleach")).toBe("Bleach tag");
    expect(getAo3Tag("Naruto (Anime)")).toBeUndefined();
  });

  it("leaves other fandoms' tags untouched", () => {
    saveAo3Tag("Naruto", "n");
    saveAo3Tag("Bleach", "b");
    renameAo3Tag("Naruto", "Naruto (Anime)");
    expect(getAo3Tag("Bleach")).toBe("b");
  });
});

describe("applyFandomRename", () => {
  it("moves a pinned fandom to the new name", () => {
    const pinned = new Set(["Naruto", "Harry Potter"]);
    const result = applyFandomRename("Naruto", "Naruto (Anime)", pinned, undefined);
    expect(result.pinnedFandoms).toEqual(new Set(["Harry Potter", "Naruto (Anime)"]));
  });

  it("leaves the pinned set reference untouched when it doesn't contain the old fandom", () => {
    const pinned = new Set(["Harry Potter"]);
    const result = applyFandomRename("Naruto", "Naruto (Anime)", pinned, undefined);
    expect(result.pinnedFandoms).toBe(pinned);
  });

  it("updates the current search fandom when it matches the old name", () => {
    const result = applyFandomRename("Naruto", "Naruto (Anime)", new Set(), "Naruto");
    expect(result.searchFandom).toBe("Naruto (Anime)");
  });

  it("leaves the current search fandom untouched when it doesn't match the old name", () => {
    const result = applyFandomRename("Naruto", "Naruto (Anime)", new Set(), "Harry Potter");
    expect(result.searchFandom).toBe("Harry Potter");
  });

  it("leaves an undefined search fandom as undefined", () => {
    const result = applyFandomRename("Naruto", "Naruto (Anime)", new Set(), undefined);
    expect(result.searchFandom).toBeUndefined();
  });
});
