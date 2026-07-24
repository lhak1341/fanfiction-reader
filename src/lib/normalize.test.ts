import { describe, expect, it } from "vitest";
import { fold } from "./normalize";
import accentFoldFixture from "../../src-tauri/tests/fixtures/accent-fold.json";

// Architecture review candidate 7: this fixture is also read by
// src-tauri/src/queries.rs's fold_matches_shared_accent_fixture test — both
// `fold()` adapters must agree on every case, not just their own.
describe("fold", () => {
  it.each(accentFoldFixture as [string, string][])("folds %j to %j", (input, expected) => {
    expect(fold(input)).toBe(expected);
  });
});
