import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  listFandoms: vi.fn(),
  listStories: vi.fn(),
  renameFandom: vi.fn()
}));
vi.mock("./ficSource", () => ({
  ficSourceAdapter: vi.fn()
}));

import { ErrorBanner } from "./errorBanner.svelte";
import * as api from "./api";
import { getAo3Tag, saveAo3Tag } from "./fandom";
import { ficSourceAdapter } from "./ficSource";
import { SearchScreen } from "./searchScreen.svelte";

function makeScreen() {
  return new SearchScreen(new ErrorBanner());
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listFandoms).mockResolvedValue([]);
  vi.mocked(api.listStories).mockResolvedValue([]);
});

describe("SearchScreen.reloadTrigger", () => {
  // Architecture review candidate 4: +page.svelte's $effect watches this
  // single value instead of enumerating each field itself — this locks in
  // that every field loadStories() reads is actually tracked here.
  it("changes when any field loadStories() reads is mutated", () => {
    const search = makeScreen();
    const mutations: Array<() => void> = [
      () => (search.fandom = "Naruto"),
      () => (search.sourceFilter = "ao3"),
      () => (search.text = "hello"),
      () => (search.minWordCount = 1000),
      () => (search.status = "complete"),
      () => (search.sortBy = "fetched"),
      () => (search.hideRead = !search.hideRead),
      () => (search.hideDismissed = !search.hideDismissed),
      () => (search.hideBookmarked = !search.hideBookmarked)
    ];

    for (const mutate of mutations) {
      const before = search.reloadTrigger;
      mutate();
      expect(search.reloadTrigger).not.toEqual(before);
    }
  });
});

describe("SearchScreen.renameFandom", () => {
  it("moves a pin to the new name and follows the current selection", async () => {
    vi.mocked(api.renameFandom).mockResolvedValue(1);

    const search = makeScreen();
    search.pinnedFandoms = new Set(["Naruto", "Bleach"]);
    search.fandom = "Naruto";

    await search.renameFandom("Naruto", "Naruto (merged)");

    expect(api.renameFandom).toHaveBeenCalledWith("Naruto", "Naruto (merged)");
    expect(search.pinnedFandoms.has("Naruto (merged)")).toBe(true);
    expect(search.pinnedFandoms.has("Naruto")).toBe(false);
    expect(search.pinnedFandoms.has("Bleach")).toBe(true);
    expect(search.fandom).toBe("Naruto (merged)");
  });

  it("leaves an unrelated selection and unpinned fandoms alone", async () => {
    vi.mocked(api.renameFandom).mockResolvedValue(1);

    const search = makeScreen();
    search.pinnedFandoms = new Set();
    search.fandom = "Bleach";

    await search.renameFandom("Naruto", "Naruto (merged)");

    expect(search.fandom).toBe("Bleach");
    expect(search.pinnedFandoms.size).toBe(0);
  });

  it("moves the saved AO3 tag to the new name", async () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, value);
      }
    });
    vi.mocked(api.renameFandom).mockResolvedValue(1);
    saveAo3Tag("Naruto", "Naruto tag");

    await makeScreen().renameFandom("Naruto", "Naruto (merged)");

    expect(getAo3Tag("Naruto (merged)")).toBe("Naruto tag");
    expect(getAo3Tag("Naruto")).toBeUndefined();
    vi.unstubAllGlobals();
  });
});

describe("SearchScreen.loadMore", () => {
  it("continues with the explicitly scoped source when one is set", async () => {
    const loadMore = vi.fn().mockResolvedValue({ fandom: "Naruto", count: 5 });
    vi.mocked(ficSourceAdapter).mockReturnValue({ loadMore });

    const search = makeScreen();
    search.fandom = "Naruto";
    search.sourceFilter = "ao3";
    search.lastSearchSource = "ffn";

    await search.loadMore();

    expect(ficSourceAdapter).toHaveBeenCalledWith("ao3");
  });

  it("falls back to the last modal-search source when scoped to All", async () => {
    const loadMore = vi.fn().mockResolvedValue({ fandom: "Naruto", count: 5 });
    vi.mocked(ficSourceAdapter).mockReturnValue({ loadMore });

    const search = makeScreen();
    search.fandom = "Naruto";
    search.sourceFilter = undefined;
    search.lastSearchSource = "ao3";

    await search.loadMore();

    expect(ficSourceAdapter).toHaveBeenCalledWith("ao3");
  });

  it("does nothing without a selected fandom", async () => {
    const search = makeScreen();
    search.fandom = undefined;

    await search.loadMore();

    expect(ficSourceAdapter).not.toHaveBeenCalled();
  });
});
