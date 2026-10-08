import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChapterListItem, EnsuredChapter, StoryRow } from "./types";

vi.mock("./api", () => ({
  listBookmarks: vi.fn(),
  listBookmarkedFandoms: vi.fn(),
  getContinueReading: vi.fn(),
  fetchChapterAndWait: vi.fn(),
  ensureChapter: vi.fn(),
  listChapters: vi.fn()
}));

import * as api from "./api";
import { BookmarksScreen } from "./bookmarksScreen.svelte";

// `api.ensureChapter` resolving means the chapter is cached (an empty page
// rejects in Rust), so the download loop only needs a resolved value.
const fetchedOk = (chapters: ChapterListItem[]): EnsuredChapter => ({
  wasFetched: true,
  chapterCount: chapters.length,
  chapters,
  content: { title: "", content: "", isCached: true }
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
  lastScrollPct: null,
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

  it("clears the job once done, allowing a subsequent call", async () => {
    const bookmarks = makeScreen();
    await bookmarks.refresh();
    expect(bookmarks.job).toBeNull();
    expect(bookmarks.busy).toBe(false);

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
    expect(bookmarks.job).toBeNull();
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
    vi.mocked(api.ensureChapter).mockResolvedValue(fetchedOk(chapters));

    await makeScreen().downloadAllChapters(story);

    expect(vi.mocked(api.ensureChapter)).toHaveBeenCalledWith(story.id, 2, true);
  });

  it("only fetches chapters that aren't already cached", async () => {
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    const bookmarks = makeScreen();
    const totals: number[] = [];
    vi.mocked(api.ensureChapter).mockImplementation(async () => {
      totals.push(bookmarks.progress.total);
      return fetchedOk(chapters);
    });

    await bookmarks.downloadAllChapters(story);

    expect(vi.mocked(api.ensureChapter).mock.calls.map((c) => c[1])).toEqual([2, 3]);
    expect(totals).toEqual([2, 2]);
  });

  it("is a no-op re-entry while a download is already in flight for another story", async () => {
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(api.ensureChapter).mockResolvedValue(fetchedOk(chapters));

    const bookmarks = makeScreen();
    const first = bookmarks.downloadAllChapters(story);
    const second = bookmarks.downloadAllChapters({ ...story, id: 2 });
    await Promise.all([first, second]);

    // Second call's story never became the job — only story 1's
    // two missing chapters were ever fetched, none for story 2.
    expect(vi.mocked(api.ensureChapter)).toHaveBeenCalledTimes(2);
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
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);
    const doneAtCall: number[] = [];
    vi.mocked(api.ensureChapter).mockImplementation(async () => {
      doneAtCall.push(bookmarks.progress.done);
      if (doneAtCall.length === 1) throw new Error("Cloudflare challenge page");
      return fetchedOk(chapters);
    });

    const run = bookmarks.downloadAllChapters(story);
    await vi.advanceTimersByTimeAsync(60_000);
    await run;

    // ch 1 failed, ch 2 ok, then ch 1 retried.
    expect(vi.mocked(api.ensureChapter).mock.calls.map((c) => c[1])).toEqual([1, 2, 1]);
    expect(onError.mock.calls.filter(([m]) => m !== "")).toEqual([]);
    // ch 1 failed (done 0), ch 2 landed (done 1), the retry of ch 1 sees done 1.
    expect(doneAtCall).toEqual([0, 0, 1]);
    expect(bookmarks.job).toBeNull();
    vi.useRealTimers();
  });

  it("reports which chapters are still missing, and why, after the retries run out", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(api.ensureChapter).mockImplementation(async (_id, num) => {
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
    expect(bookmarks.job).toBeNull();
    vi.useRealTimers();
  });

  it("is a silent no-op while another job is running", async () => {
    const onError = vi.fn();
    const bookmarks = new BookmarksScreen(onError, false);
    bookmarks.job = { kind: "refresh" };

    await bookmarks.downloadAllChapters(story);
    await bookmarks.refresh();

    expect(vi.mocked(api.listChapters)).not.toHaveBeenCalled();
    expect(vi.mocked(api.listBookmarks)).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
    expect(bookmarks.job).toEqual({ kind: "refresh" });
  });
});

describe("BookmarksScreen job state", () => {
  it("is busy with a refresh job while refreshing, then idle with progress cleared", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listBookmarks).mockResolvedValueOnce([story]);
    const bookmarks = makeScreen();
    let seen: { job: unknown; busy: boolean; label: string } | undefined;
    vi.mocked(api.fetchChapterAndWait).mockImplementation(async () => {
      seen = { job: bookmarks.job, busy: bookmarks.busy, label: bookmarks.progress.label };
    });

    const run = bookmarks.refresh();
    await vi.runAllTimersAsync();
    await run;

    expect(seen).toEqual({ job: { kind: "refresh" }, busy: true, label: "" });
    expect(bookmarks.busy).toBe(false);
    expect(bookmarks.progress).toEqual({ label: "", done: 0, total: 0 });
    vi.useRealTimers();
  });

  it("names the story being downloaded and reports done/total, then clears both", async () => {
    vi.useFakeTimers();
    const chapters: ChapterListItem[] = [
      { chapterNum: 1, title: "Ch 1", isCached: false, sourceChapterId: null },
      { chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null }
    ];
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    const bookmarks = makeScreen();
    const seen: Array<{ job: unknown; progress: unknown }> = [];
    vi.mocked(api.ensureChapter).mockImplementation(async () => {
      seen.push({ job: bookmarks.job, progress: { ...bookmarks.progress } });
      return fetchedOk(chapters);
    });

    const run = bookmarks.downloadAllChapters(story);
    await vi.runAllTimersAsync();
    await run;

    const job = { kind: "download", storyId: story.id, title: story.title };
    expect(seen).toEqual([
      { job, progress: { label: "", done: 0, total: 2 } },
      { job, progress: { label: "Downloading 1 of 2…", done: 1, total: 2 } }
    ]);
    expect(bookmarks.job).toBeNull();
    expect(bookmarks.progress).toEqual({ label: "", done: 0, total: 0 });
    vi.useRealTimers();
  });
});

describe("BookmarksScreen background fetch pause", () => {
  // +page.svelte pauses these loops while Reader is shown — they keep running
  // behind it otherwise and compete for the one FFN webview.
  it("refresh() holds its fetches while paused, keeps the job set, and finishes after resume", async () => {
    vi.useFakeTimers();
    vi.mocked(api.listBookmarks).mockResolvedValueOnce([story, { ...story, id: 2 }]);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValue(undefined);
    const bookmarks = makeScreen();
    bookmarks.pauseBackgroundFetches();

    const run = bookmarks.refresh();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(api.fetchChapterAndWait)).not.toHaveBeenCalled();
    expect(bookmarks.job).toEqual({ kind: "refresh" });

    bookmarks.resumeBackgroundFetches();
    await vi.advanceTimersByTimeAsync(10_000);
    await run;

    expect(vi.mocked(api.fetchChapterAndWait).mock.calls.map((c) => c[0])).toEqual([1, 2]);
    expect(bookmarks.job).toBeNull();
    vi.useRealTimers();
  });

  it("downloadAllChapters() stops between chapters when paused mid-download, then continues", async () => {
    vi.useFakeTimers();
    const chapters: ChapterListItem[] = [
      { chapterNum: 1, title: "Ch 1", isCached: false, sourceChapterId: null },
      { chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null }
    ];
    vi.mocked(api.listChapters).mockResolvedValue(chapters);
    vi.mocked(api.ensureChapter).mockResolvedValue(fetchedOk(chapters));
    const bookmarks = makeScreen();

    const run = bookmarks.downloadAllChapters(story);
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.mocked(api.ensureChapter)).toHaveBeenCalledTimes(1);

    bookmarks.pauseBackgroundFetches();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(vi.mocked(api.ensureChapter)).toHaveBeenCalledTimes(1);
    expect(bookmarks.job).toMatchObject({ kind: "download", storyId: story.id });

    bookmarks.resumeBackgroundFetches();
    await vi.advanceTimersByTimeAsync(10_000);
    await run;

    expect(vi.mocked(api.ensureChapter)).toHaveBeenCalledTimes(2);
    expect(bookmarks.job).toBeNull();
    vi.useRealTimers();
  });
});
