import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  listFandomIndex: vi.fn(),
  indexFfnCategoryAndWait: vi.fn(),
  searchFfnAndWait: vi.fn(),
  searchAo3AndWait: vi.fn(),
  addStoryByUrl: vi.fn(),
  toggleBookmarked: vi.fn()
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
    vi.useFakeTimers();
    vi.mocked(api.indexFfnCategoryAndWait).mockResolvedValue({ category: "anime", count: 10 });

    const screen = makeScreen();
    screen.category = ALL;
    const run = screen.refreshIndex();
    await vi.runAllTimersAsync();
    await run;

    expect(vi.mocked(api.indexFfnCategoryAndWait).mock.calls.map((c) => c[0])).toEqual(
      FFN_CATEGORIES.map((c) => c.value)
    );
    expect(api.listFandomIndex).toHaveBeenCalledWith(undefined);
    expect(screen.indexError).toBe("");
    expect(screen.indexing).toBe(false);
    vi.useRealTimers();
  });

  it("retries a failed category once and reports the ones that never indexed", async () => {
    vi.useFakeTimers();
    const bad = FFN_CATEGORIES[1].value;
    vi.mocked(api.indexFfnCategoryAndWait).mockImplementation(async (cat) => {
      if (cat === bad) throw new Error("Cloudflare challenge page");
      return { category: cat, count: 1 };
    });

    const screen = makeScreen();
    screen.category = ALL;
    const run = screen.refreshIndex();
    await vi.runAllTimersAsync();
    await run;

    const badCalls = vi.mocked(api.indexFfnCategoryAndWait).mock.calls.filter((c) => c[0] === bad);
    expect(badCalls).toHaveLength(2);
    expect(screen.indexError).toContain("1 categories failed to index");
    expect(screen.indexError).toContain(FFN_CATEGORIES[1].label);
    expect(screen.indexError).toContain("Cloudflare challenge page");
    expect(screen.indexing).toBe(false);
    vi.useRealTimers();
  });

  it("indexes only the selected category otherwise", async () => {
    vi.mocked(api.indexFfnCategoryAndWait).mockResolvedValue({ category: "anime", count: 10 });

    const screen = makeScreen();
    screen.category = "anime";
    await screen.refreshIndex();

    expect(api.indexFfnCategoryAndWait).toHaveBeenCalledWith("anime");
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
    screen.mode = "ffn";
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
    screen.mode = "ffn";
    screen.selectedFandom = undefined;

    await screen.runSearch();

    expect(api.searchFfnAndWait).not.toHaveBeenCalled();
    expect(onImported).not.toHaveBeenCalled();
    expect(screen.searching).toBe(false);
  });

  it("sets error and clears searching on failure", async () => {
    vi.mocked(api.searchFfnAndWait).mockRejectedValue(new Error("cloudflare blocked"));
    const screen = makeScreen();
    screen.mode = "ffn";
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
    screen.mode = "ao3";
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
    screen.mode = "ao3";
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
    screen.mode = "ffn";
    expect(screen.canSubmit).toBe(false);
    screen.selectedFandom = fandomEntry();
    expect(screen.canSubmit).toBe(true);
  });

  it("requires both fandom name and tag for ao3", () => {
    const screen = makeScreen();
    screen.mode = "ao3";
    expect(screen.canSubmit).toBe(false);
    screen.ao3FandomName = "Naruto";
    expect(screen.canSubmit).toBe(false);
    screen.ao3Tag = "Naruto (Anime & Manga)";
    expect(screen.canSubmit).toBe(true);
  });
});

describe("SearchModalScreen.runSearch (url)", () => {
  it("adds the story by URL, bookmarks it, and reports it added", async () => {
    vi.mocked(api.addStoryByUrl).mockResolvedValue({ storyId: 7, chapterNum: 1 });
    vi.mocked(api.toggleBookmarked).mockResolvedValue(undefined);
    const onImported = vi.fn();
    const onStoryAdded = vi.fn();
    const screen = new SearchModalScreen(onImported, onStoryAdded);
    screen.mode = "url";
    screen.url = "  https://www.fanfiction.net/s/14583034/1/Harry-Potter-The-Game  ";

    await screen.runSearch();

    expect(api.addStoryByUrl).toHaveBeenCalledWith("https://www.fanfiction.net/s/14583034/1/Harry-Potter-The-Game");
    expect(api.toggleBookmarked).toHaveBeenCalledWith(7, true);
    expect(onStoryAdded).toHaveBeenCalledTimes(1);
    expect(onImported).not.toHaveBeenCalled();
    expect(screen.searching).toBe(false);
  });

  it("canSubmit needs a non-blank URL in url mode", () => {
    const screen = new SearchModalScreen(vi.fn());
    screen.mode = "url";
    expect(screen.canSubmit).toBe(false);
    screen.url = "   ";
    expect(screen.canSubmit).toBe(false);
    screen.url = "https://archiveofourown.org/works/1";
    expect(screen.canSubmit).toBe(true);
  });

  it("does nothing for a blank URL", async () => {
    const screen = new SearchModalScreen(vi.fn());
    screen.mode = "url";
    screen.url = "  ";
    await screen.runSearch();
    expect(api.addStoryByUrl).not.toHaveBeenCalled();
  });

  it("surfaces a fetch/validation error and does not report an add", async () => {
    vi.mocked(api.addStoryByUrl).mockRejectedValue("not a valid URL");
    const onStoryAdded = vi.fn();
    const screen = new SearchModalScreen(vi.fn(), onStoryAdded);
    screen.mode = "url";
    screen.url = "nope";

    await screen.runSearch();

    expect(screen.error).toBe("not a valid URL");
    expect(api.toggleBookmarked).not.toHaveBeenCalled();
    expect(onStoryAdded).not.toHaveBeenCalled();
    expect(screen.searching).toBe(false);
  });

  it("says the import succeeded when only the bookmark step fails, and does not report an add", async () => {
    vi.mocked(api.addStoryByUrl).mockResolvedValue({ storyId: 7, chapterNum: 1 });
    vi.mocked(api.toggleBookmarked).mockRejectedValue(new Error("db locked"));
    const onStoryAdded = vi.fn();
    const screen = new SearchModalScreen(vi.fn(), onStoryAdded);
    screen.mode = "url";
    screen.url = "https://www.fanfiction.net/s/1/1/x";

    await screen.runSearch();

    expect(screen.error).toBe("Imported, but couldn't bookmark it: db locked");
    expect(onStoryAdded).not.toHaveBeenCalled();
    expect(screen.searching).toBe(false);
  });
});
