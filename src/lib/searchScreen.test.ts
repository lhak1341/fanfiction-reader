import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  listFandoms: vi.fn(),
  listStories: vi.fn(),
  renameFandom: vi.fn()
}));
vi.mock("./ficSource", () => ({
  ficSourceAdapter: vi.fn()
}));

import * as api from "./api";
import { ficSourceAdapter } from "./ficSource";
import { SearchScreen } from "./searchScreen.svelte";

function makeScreen() {
  return new SearchScreen(() => {});
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listFandoms).mockResolvedValue([]);
  vi.mocked(api.listStories).mockResolvedValue([]);
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
});

describe("SearchScreen.loadMore", () => {
  it("continues with the explicitly scoped source when one is set", async () => {
    const loadMore = vi.fn().mockResolvedValue({ fandom: "Naruto", count: 5 });
    vi.mocked(ficSourceAdapter).mockReturnValue({ chapterUrl: vi.fn(), loadMore });

    const search = makeScreen();
    search.fandom = "Naruto";
    search.sourceFilter = "ao3";
    search.lastSearchSource = "ffn";

    await search.loadMore();

    expect(ficSourceAdapter).toHaveBeenCalledWith("ao3");
  });

  it("falls back to the last modal-search source when scoped to All", async () => {
    const loadMore = vi.fn().mockResolvedValue({ fandom: "Naruto", count: 5 });
    vi.mocked(ficSourceAdapter).mockReturnValue({ chapterUrl: vi.fn(), loadMore });

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
