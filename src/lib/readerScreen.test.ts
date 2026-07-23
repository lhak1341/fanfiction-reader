import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChapterListItem, StoryRow } from "./types";

vi.mock("./api", () => ({
  getStory: vi.fn(),
  listChapters: vi.fn(),
  setResumeChapter: vi.fn(),
  toggleRead: vi.fn(),
  toggleBookmarked: vi.fn(),
  toggleDismissed: vi.fn()
}));
vi.mock("./chapterCache", () => ({
  ensureChapterCached: vi.fn()
}));
vi.mock("./pacedLoop", () => ({
  sleep: vi.fn(() => Promise.resolve()),
  jitterDelay: vi.fn(() => 0)
}));

import * as api from "./api";
import { ensureChapterCached } from "./chapterCache";
import { sleep } from "./pacedLoop";
import { ReaderScreen } from "./readerScreen.svelte";

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
  chapterCount: 20,
  summary: null,
  isRead: false,
  dismissed: false,
  bookmarked: false,
  lastChapterNum: null,
  ffnUpdatedAt: null,
  source: "ffn",
  cachedChapterCount: 0
};

function makeChapters(count: number): ChapterListItem[] {
  return Array.from({ length: count }, (_, i) => ({ chapterNum: i + 1, title: `Ch ${i + 1}`, isCached: false, sourceChapterId: null }));
}

function makeScreen(chapterCount = story.chapterCount) {
  vi.mocked(api.getStory).mockResolvedValue({ ...story, chapterCount });
  vi.mocked(api.listChapters).mockResolvedValue(makeChapters(chapterCount));
  return new ReaderScreen(story.id);
}

// Flushes both microtasks and any queued setTimeout(0)-based work (our
// mocked `sleep` resolves immediately, but still queues a microtask hop) —
// without a macrotask flush a still-running prefetch chain can outlive the
// test and its assertions.
function flush() {
  return new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.setResumeChapter).mockResolvedValue(undefined);
  vi.mocked(ensureChapterCached).mockResolvedValue({ content: null, chapters: makeChapters(story.chapterCount), wasFetched: true });
});

describe("ReaderScreen.loadChapter", () => {
  it("sets fetchError when called before story metadata has loaded", async () => {
    const screen = new ReaderScreen(story.id);
    await screen.loadChapter(1);
    expect(screen.fetchError).toBe("story metadata not loaded yet");
    expect(screen.loading).toBe(false);
  });

  it("sets fetchError and clears loading when the fetch fails", async () => {
    const screen = makeScreen();
    await screen.init(1);

    vi.mocked(ensureChapterCached).mockRejectedValueOnce(new Error("network fell over"));
    await screen.loadChapter(2);

    expect(screen.fetchError).toBe("network fell over");
    expect(screen.loading).toBe(false);
  });
});

describe("ReaderScreen.goToChapter", () => {
  it("no-ops below chapter 1 or above chapterCount", async () => {
    const screen = makeScreen(3);
    await screen.init(2);
    const callsBefore = vi.mocked(ensureChapterCached).mock.calls.length;

    await screen.goToChapter(0);
    await screen.goToChapter(4);

    expect(screen.currentChapterNum).toBe(2);
    expect(vi.mocked(ensureChapterCached).mock.calls.length).toBe(callsBefore);
  });
});

describe("ReaderScreen prefetch chain", () => {
  it("bails a stale prefetch chain once a real navigation supersedes it", async () => {
    const calls: number[] = [];
    let callCount = 0;
    let releaseGate!: () => void;
    const gate = new Promise<void>((r) => (releaseGate = r));

    vi.mocked(ensureChapterCached).mockImplementation(async (_story, _chapters, _storyId, num) => {
      callCount++;
      calls.push(num);
      // Pauses exactly the second call — the first target (chapter 2) of
      // the center=1 prefetch chain kicked off by init(1) below — so the
      // test can supersede it with a real navigation before it resolves.
      if (callCount === 2) await gate;
      return { content: null, chapters: makeChapters(20), wasFetched: true };
    });

    const screen = makeScreen(20);
    await screen.init(1); // loadChapter(1) resolves; its prefetchNeighbors(gen=1, center=1) chain
    // has started and is now paused mid-fetch of chapter 2 (calls: [1, 2]).
    expect(calls).toEqual([1, 2]);

    // A real navigation to chapter 10 bumps the generation token. Its own
    // prefetch chain (center=10 -> targets 8/9/11/12) never touches chapter
    // 3, so any chapter-3 fetch afterward could only come from the stale
    // center=1 chain's second target.
    await screen.loadChapter(10);

    releaseGate();
    await flush();

    expect(calls.filter((n) => n === 3)).toEqual([]);
  });

  it("skips pacing on an already-cached neighbor but paces on a real fetch", async () => {
    vi.mocked(ensureChapterCached).mockImplementation(async (_story, _chapters, _storyId, num) => ({
      content: null,
      chapters: makeChapters(5),
      // Center chapter 3 in a 5-chapter story prefetches [1, 2, 4, 5];
      // chapter 1 alone reports as already cached.
      wasFetched: num !== 1
    }));

    const screen = makeScreen(5);
    await screen.init(3);
    await flush();

    expect(vi.mocked(sleep)).toHaveBeenCalledTimes(3); // chapters 2, 4, 5 — not the skipped chapter 1
  });
});

describe("ReaderScreen toggle methods", () => {
  it("toggleRead flips isRead, persists it, and notifies the caller", async () => {
    const onChanged = vi.fn();
    vi.mocked(api.getStory).mockResolvedValue({ ...story, isRead: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    const screen = new ReaderScreen(story.id, onChanged);
    await screen.init(1);
    onChanged.mockClear();

    await screen.toggleRead();

    expect(api.toggleRead).toHaveBeenCalledWith(story.id, true);
    expect(screen.story?.isRead).toBe(true);
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it("toggleBookmark flips bookmarked and persists it", async () => {
    vi.mocked(api.getStory).mockResolvedValue({ ...story, bookmarked: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    const screen = new ReaderScreen(story.id);
    await screen.init(1);

    await screen.toggleBookmark();

    expect(api.toggleBookmarked).toHaveBeenCalledWith(story.id, true);
    expect(screen.story?.bookmarked).toBe(true);
  });

  it("toggleDismiss flips dismissed and persists it", async () => {
    vi.mocked(api.getStory).mockResolvedValue({ ...story, dismissed: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    const screen = new ReaderScreen(story.id);
    await screen.init(1);

    await screen.toggleDismiss();

    expect(api.toggleDismissed).toHaveBeenCalledWith(story.id, true);
    expect(screen.story?.dismissed).toBe(true);
  });

  it("sets toggleError and leaves story/onChanged untouched when toggleRead fails", async () => {
    const onChanged = vi.fn();
    vi.mocked(api.getStory).mockResolvedValue({ ...story, isRead: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    vi.mocked(api.toggleRead).mockRejectedValueOnce(new Error("db locked"));
    const screen = new ReaderScreen(story.id, onChanged);
    await screen.init(1);
    onChanged.mockClear();

    await screen.toggleRead();

    expect(screen.toggleError).toBe("db locked");
    expect(screen.story?.isRead).toBe(false);
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("sets toggleError when toggleBookmark fails", async () => {
    vi.mocked(api.getStory).mockResolvedValue({ ...story, bookmarked: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    vi.mocked(api.toggleBookmarked).mockRejectedValueOnce(new Error("network fell over"));
    const screen = new ReaderScreen(story.id);
    await screen.init(1);

    await screen.toggleBookmark();

    expect(screen.toggleError).toBe("network fell over");
    expect(screen.story?.bookmarked).toBe(false);
  });

  it("sets toggleError when toggleDismiss fails", async () => {
    vi.mocked(api.getStory).mockResolvedValue({ ...story, dismissed: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    vi.mocked(api.toggleDismissed).mockRejectedValueOnce(new Error("network fell over"));
    const screen = new ReaderScreen(story.id);
    await screen.init(1);

    await screen.toggleDismiss();

    expect(screen.toggleError).toBe("network fell over");
    expect(screen.story?.dismissed).toBe(false);
  });

  it("clears a previous toggleError on the next successful toggle", async () => {
    vi.mocked(api.getStory).mockResolvedValue({ ...story, isRead: false });
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(1));
    vi.mocked(api.toggleRead).mockRejectedValueOnce(new Error("db locked"));
    const screen = new ReaderScreen(story.id);
    await screen.init(1);

    await screen.toggleRead();
    expect(screen.toggleError).toBe("db locked");

    await screen.toggleRead();
    expect(screen.toggleError).toBe("");
    expect(screen.story?.isRead).toBe(true);
  });
});
