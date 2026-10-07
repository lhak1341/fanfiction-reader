import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChapterContent, ChapterListItem, StoryRow } from "./types";

vi.mock("./api", () => ({
  listBookmarks: vi.fn(),
  listBookmarkedFandoms: vi.fn(),
  getContinueReading: vi.fn(),
  fetchChapterAndWait: vi.fn(),
  listChapters: vi.fn()
}));
vi.mock("./chapterCache", () => ({
  ensureChapterCached: vi.fn()
}));

import * as api from "./api";
import { ensureChapterCached } from "./chapterCache";
import { BookmarksScreen } from "./bookmarksScreen.svelte";

const fetchedOk = (chapters: ChapterListItem[]) => ({
  content: { isCached: true } as ChapterContent,
  chapters,
  wasFetched: true
});

const story: StoryRow = {
  id: 1,
  sourceStoryId: 123,
  title: "t",
  author: "a",
  fandom: "f",
  rating: "K",
  genre: null,
  language: null,
  tags: null,
  status: "in_progress",
  wordCount: 0,
  favoriteCount: 0,
  chapterCount: 5,
  summary: null,
  isRead: false,
  dismissed: false,
  bookmarked: true,
  lastChapterNum: null,
  ffnUpdatedAt: null,
  source: "ffn",
  cachedChapterCount: 0
};

function makeScreen() {
  return new BookmarksScreen(() => {}, false);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.listBookmarks).mockResolvedValue([]);
  vi.mocked(api.listBookmarkedFandoms).mockResolvedValue([]);
  vi.mocked(api.getContinueReading).mockResolvedValue([]);
});

describe("BookmarksScreen.reloadTrigger", () => {
  // Architecture review candidate 4: +page.svelte's $effect watches this
  // single value instead of enumerating each field itself.
  it("changes when any field loadStories() reads is mutated", () => {
    const bookmarks = makeScreen();
    const mutations: Array<() => void> = [
      () => (bookmarks.fandom = "Naruto"),
      () => (bookmarks.sortBy = "updated"),
      () => (bookmarks.hideRead = true)
    ];

    for (const mutate of mutations) {
      const before = bookmarks.reloadTrigger;
      mutate();
      expect(bookmarks.reloadTrigger).not.toEqual(before);
    }
  });
});

describe("BookmarksScreen.refresh", () => {
  it("is a no-op re-entry while a refresh is already in flight", async () => {
    const bookmarks = makeScreen();
    const first = bookmarks.refresh();
    const second = bookmarks.refresh();
    await Promise.all([first, second]);

    // One call for the in-progress-story fetch inside refresh(), plus the
    // reloadAll() at the end — a concurrent second call must add none.
    expect(vi.mocked(api.listBookmarks)).toHaveBeenCalledTimes(2);
  });

  it("clears the refreshing flag once done, allowing a subsequent call", async () => {
    const bookmarks = makeScreen();
    await bookmarks.refresh();
    expect(bookmarks.refreshing).toBe(false);

    await bookmarks.refresh();
    expect(vi.mocked(api.listBookmarks)).toHaveBeenCalledTimes(4);
  });
});

describe("BookmarksScreen.refresh failures", () => {
  it("fetches in the background", async () => {
    vi.mocked(api.listBookmarks).mockResolvedValueOnce([story]);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValue(undefined);

    await makeScreen().refresh();

    expect(vi.mocked(api.fetchChapterAndWait)).toHaveBeenCalledWith(story.id, 1, true);
  });

  it("reports how many stories failed to refresh and why, without retrying", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listBookmarks).mockResolvedValueOnce([story, { ...story, id: 2 }, { ...story, id: 3 }]);
    vi.mocked(api.fetchChapterAndWait).mockImplementation(async (id) => {
      if (id !== 2) throw new Error("fetch timed out");
    });
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);

    const run = bookmarks.refresh();
    await vi.runAllTimersAsync();
    await run;

    expect(vi.mocked(api.fetchChapterAndWait)).toHaveBeenCalledTimes(3);
    expect(onError).toHaveBeenLastCalledWith("2 stories failed to refresh: Last error: fetch timed out");
    expect(bookmarks.refreshing).toBe(false);
    vi.useRealTimers();
  });
});

describe("BookmarksScreen.downloadAllChapters", () => {
  const chapters: ChapterListItem[] = [
    { chapterNum: 1, title: "Ch 1", isCached: true, sourceChapterId: null },
    { chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null },
    { chapterNum: 3, title: "Ch 3", isCached: false, sourceChapterId: null }
  ];

  it("fetches in the background so it never interrupts a foreground fetch", async () => {
    vi.mocked(api.listChapters).mockResolvedValue([chapters[1]]);
    vi.mocked(ensureChapterCached).mockResolvedValue(fetchedOk(chapters));

    await makeScreen().downloadAllChapters(story);

    expect(vi.mocked(ensureChapterCached)).toHaveBeenCalledWith(story.id, 2, { background: true });
  });

  it("only fetches chapters that aren't already cached", async () => {
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached).mockResolvedValue(fetchedOk(chapters));

    const bookmarks = makeScreen();
    await bookmarks.downloadAllChapters(story);

    expect(vi.mocked(ensureChapterCached).mock.calls.map((c) => c[1])).toEqual([2, 3]);
    expect(bookmarks.downloadTotal).toBe(2);
  });

  it("is a no-op re-entry while a download is already in flight for another story", async () => {
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached).mockResolvedValue(fetchedOk(chapters));

    const bookmarks = makeScreen();
    const first = bookmarks.downloadAllChapters(story);
    const second = bookmarks.downloadAllChapters({ ...story, id: 2 });
    await Promise.all([first, second]);

    // Second call's story id never became downloadingStoryId — only story 1's
    // two missing chapters were ever fetched, none for story 2.
    expect(vi.mocked(ensureChapterCached)).toHaveBeenCalledTimes(2);
  });
});

describe("BookmarksScreen.downloadAllChapters failures", () => {
  const chapters: ChapterListItem[] = [
    { chapterNum: 1, title: "Ch 1", isCached: false, sourceChapterId: null },
    { chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null }
  ];

  it("retries a chapter that failed and reports nothing once it lands", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached)
      .mockRejectedValueOnce(new Error("Cloudflare challenge page"))
      .mockResolvedValue(fetchedOk(chapters));
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);

    const run = bookmarks.downloadAllChapters(story);
    await vi.advanceTimersByTimeAsync(60_000);
    await run;

    // ch 1 failed, ch 2 ok, then ch 1 retried.
    expect(vi.mocked(ensureChapterCached).mock.calls.map((c) => c[1])).toEqual([1, 2, 1]);
    expect(onError.mock.calls.filter(([m]) => m !== "")).toEqual([]);
    expect(bookmarks.downloadDone).toBe(2);
    vi.useRealTimers();
  });

  it("reports which chapters are still missing, and why, after the retries run out", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached).mockImplementation(async (_id, num) => {
      if (num === 2) throw new Error("interrupted by a newer fetch");
      return fetchedOk(chapters);
    });
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);

    const run = bookmarks.downloadAllChapters(story);
    await vi.advanceTimersByTimeAsync(120_000);
    await run;

    expect(onError).toHaveBeenLastCalledWith(expect.stringContaining("1 chapter failed (2)"));
    expect(onError).toHaveBeenLastCalledWith(expect.stringContaining("interrupted by a newer fetch"));
    expect(bookmarks.downloadingStoryId).toBeNull();
    vi.useRealTimers();
  });

  it("treats a fetch that persisted nothing as a failure", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listChapters).mockResolvedValue([chapters[0]]);
    vi.mocked(ensureChapterCached).mockResolvedValue({ content: null, chapters, wasFetched: true });
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);

    const run = bookmarks.downloadAllChapters(story);
    await vi.advanceTimersByTimeAsync(120_000);
    await run;

    expect(onError).toHaveBeenLastCalledWith(expect.stringContaining("came back empty"));
    vi.useRealTimers();
  });

  it("refuses to start while a refresh is running, and says so", async () => {
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);
    bookmarks.refreshing = true;

    await bookmarks.downloadAllChapters(story);

    expect(vi.mocked(api.listChapters)).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith(expect.stringContaining("refresh"));
  });
});

describe("BookmarksScreen background fetch pause", () => {
  // +page.svelte pauses these loops while Reader is shown — they keep running
  // behind it otherwise and compete for the one FFN webview.
  it("refresh() holds its fetches while paused, keeps `refreshing` set, and finishes after resume", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listBookmarks).mockResolvedValueOnce([story, { ...story, id: 2 }]);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValue(undefined);
    const bookmarks = makeScreen();
    bookmarks.pauseBackgroundFetches();

    const run = bookmarks.refresh();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(api.fetchChapterAndWait)).not.toHaveBeenCalled();
    expect(bookmarks.refreshing).toBe(true);

    bookmarks.resumeBackgroundFetches();
    await vi.advanceTimersByTimeAsync(10_000);
    await run;

    expect(vi.mocked(api.fetchChapterAndWait).mock.calls.map((c) => c[0])).toEqual([1, 2]);
    expect(bookmarks.refreshing).toBe(false);
    vi.useRealTimers();
  });

  it("downloadAllChapters() stops between chapters when paused mid-download, then continues", async () => {
    vi.useFakeTimers();
    const chapters: ChapterListItem[] = [
      { chapterNum: 1, title: "Ch 1", isCached: false, sourceChapterId: null },
      { chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null }
    ];
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached).mockResolvedValue(fetchedOk(chapters));
    const bookmarks = makeScreen();

    const run = bookmarks.downloadAllChapters(story);
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.mocked(ensureChapterCached)).toHaveBeenCalledTimes(1);

    bookmarks.pauseBackgroundFetches();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(ensureChapterCached)).toHaveBeenCalledTimes(1);
    expect(bookmarks.downloadingStoryId).toBe(story.id);

    bookmarks.resumeBackgroundFetches();
    await vi.advanceTimersByTimeAsync(10_000);
    await run;

    expect(vi.mocked(ensureChapterCached)).toHaveBeenCalledTimes(2);
    expect(bookmarks.downloadingStoryId).toBeNull();
    vi.useRealTimers();
  });
});
