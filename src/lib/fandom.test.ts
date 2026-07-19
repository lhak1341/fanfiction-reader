import { describe, expect, it } from "vitest";
import { applyFandomRename } from "./fandom";

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
