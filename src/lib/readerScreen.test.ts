import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChapterListItem, EnsuredChapter, StoryRow } from "./types";

vi.mock("./api", () => ({
  getStory: vi.fn(),
  listChapters: vi.fn(),
  ensureChapter: vi.fn(),
  setResumeChapter: vi.fn(),
  toggleRead: vi.fn(),
  toggleBookmarked: vi.fn(),
  toggleDismissed: vi.fn()
}));
vi.mock("./pacedLoop", () => ({
  sleep: vi.fn(() => Promise.resolve()),
  jitterDelay: vi.fn(() => 0)
}));

import * as api from "./api";
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
  lastScrollPct: null,
  ffnUpdatedAt: null,
  source: "ffn",
  cachedChapterCount: 0
};

function makeChapters(count: number): ChapterListItem[] {
  return Array.from({ length: count }, (_, i) => ({ chapterNum: i + 1, title: `Ch ${i + 1}`, isCached: false }));
}

// `api.ensureChapter`'s result: a fetch (`wasFetched`) also reports the story's
// chapter count and the refreshed list; a cache hit reports neither.
// The count defaults to the current screen's, so a fetch leaves it unchanged
// unless a test says the story grew.
let screenChapterCount = story.chapterCount;

function ensured(num: number, wasFetched = true, chapterCount = screenChapterCount): EnsuredChapter {
  return {
    wasFetched,
    chapterCount: wasFetched ? chapterCount : null,
    chapters: wasFetched ? makeChapters(chapterCount) : null,
    content: { title: `Ch ${num}`, content: "", isCached: true }
  };
}

function makeScreen(chapterCount = story.chapterCount) {
  screenChapterCount = chapterCount;
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
  screenChapterCount = story.chapterCount;
  vi.mocked(api.setResumeChapter).mockResolvedValue(undefined);
  vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => ensured(num));
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

    vi.mocked(api.ensureChapter).mockRejectedValueOnce(new Error("network fell over"));
    await screen.loadChapter(2);

    expect(screen.fetchError).toBe("network fell over");
    expect(screen.loading).toBe(false);
  });
});

describe("ReaderScreen.init failures", () => {
  it("surfaces a getStory failure as fetchError and stops loading, without rejecting", async () => {
    vi.mocked(api.getStory).mockRejectedValue(new Error("db locked"));
    const screen = new ReaderScreen(story.id);

    await expect(screen.init(1)).resolves.toBeUndefined();

    expect(screen.fetchError).toBe("Could not load story: db locked");
    expect(screen.loading).toBe(false);
    expect(api.ensureChapter).not.toHaveBeenCalled();
  });

  it("surfaces a listChapters failure the same way", async () => {
    vi.mocked(api.getStory).mockResolvedValue(story);
    vi.mocked(api.listChapters).mockRejectedValue("disk full");
    const screen = new ReaderScreen(story.id);

    await screen.init(1);

    expect(screen.fetchError).toBe("Could not load story: disk full");
    expect(screen.loading).toBe(false);
    expect(api.ensureChapter).not.toHaveBeenCalled();
  });

  it("reports a missing story as \"story not found\"", async () => {
    vi.mocked(api.getStory).mockResolvedValue(null);
    const screen = new ReaderScreen(story.id);

    await screen.init(1);

    expect(screen.fetchError).toBe("story not found");
    expect(screen.loading).toBe(false);
    expect(api.ensureChapter).not.toHaveBeenCalled();
  });
});

describe("ReaderScreen resume bookkeeping failures", () => {
  it("keeps a loaded chapter on screen, notifies the caller and still prefetches when saving the resume position fails", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(api.getStory).mockResolvedValue(story);
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(story.chapterCount));
    vi.mocked(api.ensureChapter).mockResolvedValue({
      ...ensured(1, false),
      content: { title: "Ch 1", content: "text", isCached: true }
    });
    vi.mocked(api.setResumeChapter).mockRejectedValue(new Error("db locked"));
    const onChanged = vi.fn();
    const screen = new ReaderScreen(story.id, onChanged);

    await screen.init(1);
    await flush();

    expect(screen.fetchError).toBe("");
    expect(screen.content?.content).toBe("text");
    expect(screen.currentChapterNum).toBe(1);
    expect(screen.loading).toBe(false);
    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(errors).toHaveBeenCalled();
    // The prefetch chain still ran: chapter 1 for the load, then its neighbours.
    expect(vi.mocked(api.ensureChapter).mock.calls.map((c) => c[1])).toContain(2);
    errors.mockRestore();
  });

  it("keeps a loaded chapter on screen when onChanged throws", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(api.getStory).mockResolvedValue(story);
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(story.chapterCount));
    vi.mocked(api.ensureChapter).mockResolvedValue({
      ...ensured(1, false),
      content: { title: "Ch 1", content: "text", isCached: true }
    });
    const screen = new ReaderScreen(story.id, () => {
      throw new Error("refresh blew up");
    });

    await screen.init(1);

    expect(screen.fetchError).toBe("");
    expect(screen.content?.content).toBe("text");
    errors.mockRestore();
  });
});

// Holds the first `api.ensureChapter` call for each listed chapter until
// its returned `release`/`fail` is called; every other call resolves at once.
function gateFirstCall(nums: number[]) {
  const gates = new Map<number, { release: () => void; fail: (e: Error) => void }>();
  const pending = new Set(nums);
  const fetchCalls: number[] = [];
  vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => {
    fetchCalls.push(num);
    const result = ensured(num, true, 20);
    if (!pending.delete(num)) return result;
    await new Promise<void>((resolve, reject) => gates.set(num, { release: resolve, fail: (e) => reject(e) }));
    return result;
  });
  return { gates, fetchCalls };
}

describe("ReaderScreen.loadChapter supersession", () => {
  it("drops a slow load that resolves after a newer navigation", async () => {
    const { gates } = gateFirstCall([10]);
    const screen = makeScreen(20);
    await screen.init(1);

    const slow = screen.loadChapter(10); // blocked on the gate
    await screen.loadChapter(12); // resolves at once
    gates.get(10)!.release();
    await slow;
    await flush();

    expect(screen.currentChapterNum).toBe(12);
    expect(screen.content?.title).toBe("Ch 12");
    expect(vi.mocked(api.setResumeChapter)).not.toHaveBeenCalledWith(story.id, 10, expect.anything());
  });

  it("ignores a superseded load's failure and keeps loading state for the newer one", async () => {
    const { gates } = gateFirstCall([10, 12]);
    const screen = makeScreen(20);
    await screen.init(1);

    const slow = screen.loadChapter(10);
    const newer = screen.loadChapter(12); // also gated
    gates.get(10)!.fail(new Error("stale boom"));
    await slow;

    expect(screen.fetchError).toBe("");
    expect(screen.loading).toBe(true); // newer load is still running

    gates.get(12)!.release();
    await newer;
    expect(screen.loading).toBe(false);
    expect(screen.currentChapterNum).toBe(12);
  });

  it("coalesces a repeat load of the chapter already in flight", async () => {
    const { gates, fetchCalls } = gateFirstCall([8]);
    const screen = makeScreen(20);
    await screen.init(1);

    const first = screen.loadChapter(8);
    const second = screen.loadChapter(8);
    gates.get(8)!.release();
    await Promise.all([first, second]);

    expect(fetchCalls.filter((n) => n === 8)).toHaveLength(1);
    expect(screen.currentChapterNum).toBe(8);
  });
});

describe("ReaderScreen story metadata refresh", () => {
  it("picks up a grown chapterCount after a fetch so goToChapter can reach the new chapter", async () => {
    const screen = makeScreen(3);
    await screen.init(1);
    await flush();
    await screen.goToChapter(4);
    expect(screen.currentChapterNum).toBe(1);

    // The fetch reports the story grew to 4 chapters.
    vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => ensured(num, true, 4));
    await screen.loadChapter(2);
    await flush();
    await screen.goToChapter(4);

    expect(screen.currentChapterNum).toBe(4);
  });
});

describe("ReaderScreen.goToChapter", () => {
  it("no-ops below chapter 1 or above chapterCount", async () => {
    const screen = makeScreen(3);
    await screen.init(2);
    await flush(); // let init's prefetch chain settle so the call count is stable
    const callsBefore = vi.mocked(api.ensureChapter).mock.calls.length;

    await screen.goToChapter(0);
    await screen.goToChapter(4);

    expect(screen.currentChapterNum).toBe(2);
    expect(vi.mocked(api.ensureChapter).mock.calls.length).toBe(callsBefore);
  });
});

describe("ReaderScreen prefetch chain", () => {
  it("bails a stale prefetch chain once a real navigation supersedes it", async () => {
    const calls: number[] = [];
    let callCount = 0;
    let releaseGate!: () => void;
    const gate = new Promise<void>((r) => (releaseGate = r));

    vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => {
      callCount++;
      calls.push(num);
      // Pauses exactly the second call — the first target (chapter 2) of
      // the center=1 prefetch chain kicked off by init(1) below — so the
      // test can supersede it with a real navigation before it resolves.
      if (callCount === 2) await gate;
      return ensured(num, true, 20);
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

  it("fetches the chapter you asked for in the foreground and its neighbours in the background", async () => {
    const screen = makeScreen(20);
    await screen.init(10);
    await flush();

    const calls = vi.mocked(api.ensureChapter).mock.calls;
    expect(calls[0]).toEqual([story.id, 10]);
    for (const call of calls.slice(1)) expect(call[2]).toBe(true);
  });

  it("stops the prefetch chain once the screen is disposed (Reader closed)", async () => {
    const calls: number[] = [];
    let releaseGate!: () => void;
    const gate = new Promise<void>((r) => (releaseGate = r));
    vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => {
      calls.push(num);
      // Holds the first prefetch target (chapter 8) mid-fetch.
      if (calls.length === 2) await gate;
      return ensured(num, true, 20);
    });

    const screen = makeScreen(20);
    await screen.init(10);
    expect(calls).toEqual([10, 8]);

    screen.dispose();
    releaseGate();
    await flush();

    // The fetch already running finished; nothing after it was started.
    expect(calls).toEqual([10, 8]);
  });

  it("starts no prefetch for a chapter whose load finishes after dispose", async () => {
    let releaseGate!: () => void;
    const gate = new Promise<void>((r) => (releaseGate = r));
    vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => {
      await gate;
      return ensured(num, true, 20);
    });

    const screen = makeScreen(20);
    const init = screen.init(10);
    await flush();
    screen.dispose();
    releaseGate();
    await init;
    await flush();

    expect(vi.mocked(api.ensureChapter)).toHaveBeenCalledTimes(1);
    expect(api.setResumeChapter).not.toHaveBeenCalled();
  });

  it("skips pacing on an already-cached neighbor but paces on a real fetch", async () => {
    // Center chapter 3 in a 5-chapter story prefetches [1, 2, 4, 5];
    // chapter 1 alone reports as already cached.
    vi.mocked(api.ensureChapter).mockImplementation(async (_storyId, num) => ensured(num, num !== 1, 5));

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

describe("ReaderScreen scroll position", () => {
  const resumed = { ...story, chapterCount: 20, lastChapterNum: 5, lastScrollPct: 43 };

  async function openAt(num: number, row: StoryRow = resumed) {
    vi.mocked(api.getStory).mockResolvedValue(row);
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(20));
    const screen = new ReaderScreen(story.id);
    await screen.init(num);
    await flush(); // let the prefetch chain settle so it can't interleave with timers
    vi.mocked(api.setResumeChapter).mockClear();
    return screen;
  }

  it("resumePctFor is the saved position for the chapter you last read, else the top", async () => {
    const screen = await openAt(5);
    expect(screen.resumePctFor(5)).toBe(43);
    expect(screen.resumePctFor(6)).toBe(0);
    expect(screen.resumePctFor(4)).toBe(0);
  });

  it("resumePctFor is 0 when no position was ever recorded, or this session already scrolled the chapter", async () => {
    expect((await openAt(5, { ...resumed, lastScrollPct: null })).resumePctFor(5)).toBe(0);

    const screen = await openAt(5);
    screen.scrollPositions.set(5, 1234); // the Reader restores that exact pixel position instead
    expect(screen.resumePctFor(5)).toBe(0);
  });

  it("saves the chapter's resume point with its restored position when it lands", async () => {
    vi.mocked(api.getStory).mockResolvedValue(resumed);
    vi.mocked(api.listChapters).mockResolvedValue(makeChapters(20));
    const screen = new ReaderScreen(story.id);

    await screen.init(5);
    await flush();

    expect(api.setResumeChapter).toHaveBeenCalledWith(story.id, 5, 43);
    await screen.loadChapter(6);
    expect(api.setResumeChapter).toHaveBeenCalledWith(story.id, 6, 0);
  });

  it("collapses a burst of scroll events into one save, with the last position, after the pause", async () => {
    const screen = await openAt(5);
    vi.useFakeTimers();
    try {
      screen.reportScroll(10);
      await vi.advanceTimersByTimeAsync(1000);
      screen.reportScroll(55);
      await vi.advanceTimersByTimeAsync(1400);
      expect(api.setResumeChapter).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(200);

      expect(vi.mocked(api.setResumeChapter).mock.calls).toEqual([[story.id, 5, 55]]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("flush saves the pending position at once, and only once", async () => {
    const screen = await openAt(5);
    screen.reportScroll(70);

    await screen.flush();
    await screen.flush();

    expect(vi.mocked(api.setResumeChapter).mock.calls).toEqual([[story.id, 5, 70]]);
  });

  it("dispose (Reader closing) saves the pending position", async () => {
    const screen = await openAt(5);
    screen.reportScroll(88);

    screen.dispose();
    await flush();

    expect(vi.mocked(api.setResumeChapter).mock.calls).toEqual([[story.id, 5, 88]]);
  });

  it("drops a position reported for the previous chapter once the next one lands", async () => {
    const screen = await openAt(5);
    screen.reportScroll(90); // still scrolling chapter 5...

    await screen.loadChapter(6); // ...then you move on
    await screen.flush();

    // Chapter 6 saved its own pair; chapter 5's 90 is never attached to it.
    expect(vi.mocked(api.setResumeChapter).mock.calls).toEqual([[story.id, 6, 0]]);
  });

  it("ignores scroll reports while no chapter is on screen", async () => {
    const screen = new ReaderScreen(story.id);
    screen.reportScroll(50);
    await screen.flush();
    expect(api.setResumeChapter).not.toHaveBeenCalled();
  });

  it("keeps the chapter on screen when saving the position fails", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    const screen = await openAt(5);
    vi.mocked(api.setResumeChapter).mockRejectedValue(new Error("db locked"));
    screen.reportScroll(30);

    await expect(screen.flush()).resolves.toBeUndefined();

    expect(screen.fetchError).toBe("");
    expect(errors).toHaveBeenCalled();
    errors.mockRestore();
  });
});
