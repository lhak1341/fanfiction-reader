import { beforeEach, describe, expect, it, vi } from "vitest";
import type { StoryRow } from "./types";

vi.mock("./api", () => ({
  listFandoms: vi.fn(),
  listStories: vi.fn(),
  renameFandom: vi.fn(),
  listBookmarks: vi.fn(),
  listBookmarkedFandoms: vi.fn(),
  getContinueReading: vi.fn(),
  fetchChapterAndWait: vi.fn()
}));

import * as api from "./api";
import { BookmarksScreen } from "./bookmarksScreen.svelte";
import { ErrorBanner } from "./errorBanner.svelte";
import { SearchScreen } from "./searchScreen.svelte";

describe("ErrorBanner.run", () => {
  it("reports a throw as prefix + message and returns false", async () => {
    const banner = new ErrorBanner();

    const ok = await banner.run("search.stories", "Could not load stories", async () => {
      throw new Error("db locked");
    });

    expect(ok).toBe(false);
    expect(banner.messages).toEqual([{ key: "search.stories", text: "Could not load stories: db locked" }]);
  });

  it("formats a non-Error rejection (a Tauri command's string) as-is", async () => {
    const banner = new ErrorBanner();

    await banner.run("flag", "Could not update bookmark", () => Promise.reject("no such story"));

    expect(banner.messages[0].text).toBe("Could not update bookmark: no such story");
  });

  it("clears its own key on success by default and returns true", async () => {
    const banner = new ErrorBanner();
    banner.report("search.stories", "old failure");

    const ok = await banner.run("search.stories", "x", async () => {});

    expect(ok).toBe(true);
    expect(banner.messages).toEqual([]);
  });

  it("leaves other keys alone on success", async () => {
    const banner = new ErrorBanner();
    banner.report("bookmarks.refresh", "3 stories failed to refresh");

    await banner.run("search.stories", "x", async () => {});

    expect(banner.messages).toEqual([{ key: "bookmarks.refresh", text: "3 stories failed to refresh" }]);
  });

  it("clearOn start: clears when the op begins, and does not clear what the op reports", async () => {
    const banner = new ErrorBanner();
    banner.report("bookmarks.refresh", "stale");
    let seenDuringOp: string[] = [];

    await banner.run(
      "bookmarks.refresh",
      "x",
      async () => {
        seenDuringOp = banner.messages.map((m) => m.text);
        banner.report("bookmarks.refresh", "1 story failed");
      },
      { clearOn: "start" }
    );

    expect(seenDuringOp).toEqual([]);
    expect(banner.messages).toEqual([{ key: "bookmarks.refresh", text: "1 story failed" }]);
  });

  it("clearOn success keeps the old message while the op is still running", async () => {
    const banner = new ErrorBanner();
    banner.report("search.stories", "old failure");
    let seenDuringOp: string[] = [];

    await banner.run("search.stories", "x", async () => {
      seenDuringOp = banner.messages.map((m) => m.text);
    });

    expect(seenDuringOp).toEqual(["old failure"]);
  });
});

describe("ErrorBanner messages", () => {
  it("stacks distinct keys in first-seen order and replaces a repeat in place", () => {
    const banner = new ErrorBanner();

    banner.report("search.stories", "a");
    banner.report("bookmarks.list", "b");
    banner.report("search.stories", "a2");

    expect(banner.messages).toEqual([
      { key: "search.stories", text: "a2" },
      { key: "bookmarks.list", text: "b" }
    ]);
  });

  it("dismiss removes one key and ignores an absent one", () => {
    const banner = new ErrorBanner();
    banner.report("search.stories", "a");
    banner.report("bookmarks.list", "b");

    banner.dismiss("search.stories");
    banner.dismiss("flag");

    expect(banner.messages).toEqual([{ key: "bookmarks.list", text: "b" }]);
  });
});

describe("ErrorBanner shared by Search and Bookmarks", () => {
  const story = { id: 1, title: "t", status: "in_progress" } as StoryRow;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.listFandoms).mockResolvedValue([]);
    vi.mocked(api.listStories).mockResolvedValue([]);
    vi.mocked(api.listBookmarkedFandoms).mockResolvedValue([]);
    vi.mocked(api.getContinueReading).mockResolvedValue([]);
  });

  // The page runs `bookmarks.refresh()` then `search.loadStories()`; the second
  // call used to wipe the first's failure message.
  it("a Refresh failure survives the Search reload the page runs right after it", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listBookmarks).mockResolvedValue([story]);
    vi.mocked(api.fetchChapterAndWait).mockRejectedValue(new Error("fetch timed out"));
    const banner = new ErrorBanner();
    const search = new SearchScreen(banner);
    const bookmarks = new BookmarksScreen(banner, false);

    const run = (async () => {
      await bookmarks.refresh();
      await search.loadStories();
    })();
    await vi.runAllTimersAsync();
    await run;

    expect(banner.messages).toEqual([
      { key: "bookmarks.refresh", text: "1 story failed to refresh: Last error: fetch timed out" }
    ]);
    vi.useRealTimers();
  });

  it("a Bookmarks reload does not clear a Download-all failure", async () => {
    const banner = new ErrorBanner();
    const bookmarks = new BookmarksScreen(banner, false);
    banner.report("bookmarks.download", "Download of \"t\" incomplete");

    await bookmarks.reloadAll();

    expect(banner.messages).toEqual([{ key: "bookmarks.download", text: "Download of \"t\" incomplete" }]);
  });

  it("a load failure clears when that same load next succeeds", async () => {
    const banner = new ErrorBanner();
    const search = new SearchScreen(banner);
    vi.mocked(api.listStories).mockRejectedValueOnce(new Error("db locked"));

    await search.loadStories();
    expect(banner.messages).toEqual([{ key: "search.stories", text: "Could not load stories: db locked" }]);

    await search.loadStories();
    expect(banner.messages).toEqual([]);
  });

  it("starting a bulk job clears that job's previous failure", async () => {
    vi.mocked(api.listBookmarks).mockResolvedValue([]);
    const banner = new ErrorBanner();
    const bookmarks = new BookmarksScreen(banner, false);
    banner.report("bookmarks.refresh", "2 stories failed to refresh");

    await bookmarks.refresh();

    expect(banner.messages).toEqual([]);
  });
});
