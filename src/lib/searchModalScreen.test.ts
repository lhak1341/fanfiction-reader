import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  listFandomIndex: vi.fn(),
  indexFfnCategoryAndWait: vi.fn(),
  indexAllCategoriesAndWait: vi.fn(),
  searchFfnAndWait: vi.fn(),
  searchAo3AndWait: vi.fn()
}));
vi.mock("./fandom", () => ({
  saveAo3Tag: vi.fn()
}));

import * as api from "./api";
import { saveAo3Tag } from "./fandom";
import { ALL, SearchModalScreen } from "./searchModalScreen.svelte";
import { FFN_CATEGORIES } from "./types";
import type { FandomIndexEntry } from "./types";

function makeScreen() {
  return new SearchModalScreen(vi.fn());
}

function fandomEntry(overrides: Partial<FandomIndexEntry> = {}): FandomIndexEntry {
  return { fandom: "Naruto", category: "anime", ffnCount: 100, ...overrides };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listFandomIndex).mockResolvedValue([]);
});

describe("SearchModalScreen.refreshIndex", () => {
  it("indexes every category when category is ALL", async () => {
    vi.mocked(api.indexAllCategoriesAndWait).mockResolvedValue(undefined);

    const screen = makeScreen();
    screen.category = ALL;
    await screen.refreshIndex();

    expect(api.indexAllCategoriesAndWait).toHaveBeenCalledWith(
      FFN_CATEGORIES.map((c) => c.value),
      expect.any(Function)
    );
    expect(api.indexFfnCategoryAndWait).not.toHaveBeenCalled();
    expect(api.listFandomIndex).toHaveBeenCalledWith(undefined);
    expect(screen.indexing).toBe(false);
  });

  it("indexes only the selected category otherwise", async () => {
    vi.mocked(api.indexFfnCategoryAndWait).mockResolvedValue({ category: "anime", count: 10 });

    const screen = makeScreen();
    screen.category = "anime";
    await screen.refreshIndex();

    expect(api.indexFfnCategoryAndWait).toHaveBeenCalledWith("anime");
    expect(api.indexAllCategoriesAndWait).not.toHaveBeenCalled();
    expect(api.listFandomIndex).toHaveBeenCalledWith("anime");
    expect(screen.indexing).toBe(false);
  });

  it("sets indexError and clears indexing on failure", async () => {
    vi.mocked(api.indexFfnCategoryAndWait).mockRejectedValue(new Error("boom"));

    const screen = makeScreen();
    screen.category = "anime";
    await screen.refreshIndex();

    expect(screen.indexError).toBe("boom");
    expect(screen.indexing).toBe(false);
  });
});

describe("SearchModalScreen.runSearch (ffn)", () => {
  it("imports the selected fandom and reports it", async () => {
    vi.mocked(api.searchFfnAndWait).mockResolvedValue({ fandom: "Naruto", count: 5 });
    const onImported = vi.fn();
    const screen = new SearchModalScreen(onImported);
    screen.source = "ffn";
    screen.selectedFandom = fandomEntry();
    screen.minWordCount = 1000;
    screen.status = "complete";

    await screen.runSearch();

    expect(api.searchFfnAndWait).toHaveBeenCalledWith("Naruto", 1000, "complete", "anime");
    expect(onImported).toHaveBeenCalledWith("Naruto", "ffn");
    expect(screen.searching).toBe(false);
  });

  it("does nothing without a selected fandom", async () => {
    const onImported = vi.fn();
    const screen = new SearchModalScreen(onImported);
    screen.source = "ffn";
    screen.selectedFandom = undefined;

    await screen.runSearch();

    expect(api.searchFfnAndWait).not.toHaveBeenCalled();
    expect(onImported).not.toHaveBeenCalled();
    expect(screen.searching).toBe(false);
  });

  it("sets error and clears searching on failure", async () => {
    vi.mocked(api.searchFfnAndWait).mockRejectedValue(new Error("cloudflare blocked"));
    const screen = makeScreen();
    screen.source = "ffn";
    screen.selectedFandom = fandomEntry();

    await screen.runSearch();

    expect(screen.error).toBe("cloudflare blocked");
    expect(screen.searching).toBe(false);
  });
});

describe("SearchModalScreen.runSearch (ao3)", () => {
  it("imports the typed fandom, saves the tag, and reports it", async () => {
    vi.mocked(api.searchAo3AndWait).mockResolvedValue({ fandom: "Naruto", count: 3 });
    const onImported = vi.fn();
    const screen = new SearchModalScreen(onImported);
    screen.source = "ao3";
    screen.ao3FandomName = " Naruto ";
    screen.ao3Tag = " Naruto (Anime & Manga) ";

    await screen.runSearch();

    expect(api.searchAo3AndWait).toHaveBeenCalledWith("Naruto (Anime & Manga)", "Naruto", 0, "any");
    expect(saveAo3Tag).toHaveBeenCalledWith("Naruto", "Naruto (Anime & Manga)");
    expect(onImported).toHaveBeenCalledWith("Naruto", "ao3");
  });

  it("does nothing without both fandom name and tag", async () => {
    const onImported = vi.fn();
    const screen = new SearchModalScreen(onImported);
    screen.source = "ao3";
    screen.ao3FandomName = "Naruto";
    screen.ao3Tag = "";

    await screen.runSearch();

    expect(api.searchAo3AndWait).not.toHaveBeenCalled();
    expect(onImported).not.toHaveBeenCalled();
  });
});

describe("SearchModalScreen.canSubmit", () => {
  it("requires a selected fandom for ffn", () => {
    const screen = makeScreen();
    screen.source = "ffn";
    expect(screen.canSubmit).toBe(false);
    screen.selectedFandom = fandomEntry();
    expect(screen.canSubmit).toBe(true);
  });

  it("requires both fandom name and tag for ao3", () => {
    const screen = makeScreen();
    screen.source = "ao3";
    expect(screen.canSubmit).toBe(false);
    screen.ao3FandomName = "Naruto";
    expect(screen.canSubmit).toBe(false);
    screen.ao3Tag = "Naruto (Anime & Manga)";
    expect(screen.canSubmit).toBe(true);
  });
});
