import { flushSync } from "./store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  listFandoms: vi.fn(),
  listStories: vi.fn(),
  renameFandom: vi.fn(),
  searchFfnAndWait: vi.fn(),
  searchAo3AndWait: vi.fn(),
}));

import { ErrorBanner } from "./errorBanner";
import * as api from "./api";
import { getAo3Tag, saveAo3Tag } from "./fandom";
import { SearchScreen } from "./searchScreen";

function stubLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
  });
  return store;
}

function makeScreen() {
  return new SearchScreen(new ErrorBanner());
}

afterEach(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listFandoms).mockResolvedValue([]);
  vi.mocked(api.listStories).mockResolvedValue([]);
});

describe("SearchScreen.start", () => {
  // Architecture review candidate 4: the reload effect watches one private
  // list of fields — flipping each one must reload the list, so a field
  // loadStories() reads but the list forgot fails here.
  it("reloads the list on start, then whenever a field loadStories() reads changes", () => {
    const search = makeScreen();
    search.start();
    flushSync();
    expect(api.listStories).toHaveBeenCalledTimes(1);

    const mutations: Array<() => void> = [
      () => (search.fandom = "Naruto"),
      () => (search.sourceFilter = "ao3"),
      () => (search.text = "hello"),
      () => (search.minWordCount = 1000),
      () => (search.status = "complete"),
      () => (search.sortBy = "fetched"),
      () => (search.hideRead = !search.hideRead),
      () => (search.hideDismissed = !search.hideDismissed),
      () => (search.hideBookmarked = !search.hideBookmarked),
    ];

    for (const mutate of mutations) {
      const before = vi.mocked(api.listStories).mock.calls.length;
      mutate();
      flushSync();
      expect(vi.mocked(api.listStories).mock.calls.length).toBe(before + 1);
    }
    search.dispose();
  });

  it("persists filter prefs and pinned fandoms when they change", () => {
    const store = stubLocalStorage();
    const search = makeScreen();
    search.start();
    flushSync();

    search.hideRead = true;
    search.pinnedFandoms = new Set(["Naruto"]);
    flushSync();

    expect(JSON.parse(store.get("ffn-search-filters")!)).toMatchObject({ hideRead: true });
    expect(store.get("ffn-pinned-fandoms")).toContain("Naruto");
    search.dispose();
  });

  it("stops reacting after dispose", () => {
    const search = makeScreen();
    search.start();
    flushSync();
    search.dispose();

    search.text = "hello";
    flushSync();
    expect(api.listStories).toHaveBeenCalledTimes(1);
  });

  it("start is idempotent", () => {
    const search = makeScreen();
    search.start();
    search.start();
    flushSync();
    expect(api.listStories).toHaveBeenCalledTimes(1);
    search.dispose();
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
      },
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
  // The real ficSource adapters run; only `api` is mocked, so these assert
  // the call a caller's choice of source actually produces.
  beforeEach(() => {
    stubLocalStorage();
    vi.mocked(api.searchFfnAndWait).mockResolvedValue({ fandom: "Naruto", count: 5 });
    vi.mocked(api.searchAo3AndWait).mockResolvedValue({ fandom: "Naruto", count: 5 });
    saveAo3Tag("Naruto", "Naruto (Anime)");
  });

  it("continues with the explicitly scoped source when one is set", async () => {
    const search = makeScreen();
    search.fandom = "Naruto";
    search.sourceFilter = "ao3";
    search.lastSearchSource = "ffn";

    await search.loadMore();

    expect(api.searchAo3AndWait).toHaveBeenCalledWith("Naruto (Anime)", "Naruto", 0, "any");
    expect(api.searchFfnAndWait).not.toHaveBeenCalled();
  });

  it("falls back to the last modal-search source when scoped to All", async () => {
    const search = makeScreen();
    search.fandom = "Naruto";
    search.sourceFilter = undefined;
    search.lastSearchSource = "ao3";

    await search.loadMore();

    expect(api.searchAo3AndWait).toHaveBeenCalledTimes(1);
    expect(api.searchFfnAndWait).not.toHaveBeenCalled();
  });

  it("continues FFN with the current filters and no category, then reloads the list", async () => {
    const search = makeScreen();
    search.fandom = "Naruto";
    search.minWordCount = 40000;
    search.status = "complete";

    await search.loadMore();

    expect(api.searchFfnAndWait).toHaveBeenCalledWith("Naruto", 40000, "complete");
    expect(api.listStories).toHaveBeenCalledTimes(1);
    expect(search.loadMoreError).toBe("");
  });

  it("reports a missing AO3 tag as loadMoreError and does not fetch", async () => {
    const search = makeScreen();
    search.fandom = "Bleach";
    search.sourceFilter = "ao3";

    await search.loadMore();

    expect(search.loadMoreError).toBe(
      'No AO3 tag saved for "Bleach" yet — search it once via the modal first.'
    );
    expect(api.searchAo3AndWait).not.toHaveBeenCalled();
    expect(search.loadingMore).toBe(false);
  });

  it("does nothing without a selected fandom", async () => {
    const search = makeScreen();
    search.fandom = undefined;

    await search.loadMore();

    expect(api.searchFfnAndWait).not.toHaveBeenCalled();
    expect(api.searchAo3AndWait).not.toHaveBeenCalled();
  });
});
