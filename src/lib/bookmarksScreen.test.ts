import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChapterListItem, StoryRow } from "./types";

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

describe("BookmarksScreen.downloadAllChapters", () => {
  const chapters: ChapterListItem[] = [
    { chapterNum: 1, title: "Ch 1", isCached: true, sourceChapterId: null },
    { chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null },
    { chapterNum: 3, title: "Ch 3", isCached: false, sourceChapterId: null }
  ];

  it("only fetches chapters that aren't already cached", async () => {
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached).mockResolvedValue({ content: null, chapters, wasFetched: true });

    const bookmarks = makeScreen();
    await bookmarks.downloadAllChapters(story);

    expect(vi.mocked(ensureChapterCached).mock.calls.map((c) => c[1])).toEqual([2, 3]);
    expect(bookmarks.downloadTotal).toBe(2);
  });

  it("is a no-op re-entry while a download is already in flight for another story", async () => {
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(ensureChapterCached).mockResolvedValue({ content: null, chapters, wasFetched: true });

    const bookmarks = makeScreen();
    const first = bookmarks.downloadAllChapters(story);
    const second = bookmarks.downloadAllChapters({ ...story, id: 2 });
    await Promise.all([first, second]);

    // Second call's story id never became downloadingStoryId — only story 1's
    // two missing chapters were ever fetched, none for story 2.
    expect(vi.mocked(ensureChapterCached)).toHaveBeenCalledTimes(2);
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
    vi.mocked(ensureChapterCached).mockResolvedValue({ content: null, chapters, wasFetched: true });
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
