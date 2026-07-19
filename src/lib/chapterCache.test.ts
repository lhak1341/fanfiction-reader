import { describe, expect, it, vi } from "vitest";
import { ensureChapterCached } from "./chapterCache";
import type { ChapterContent, ChapterListItem, StoryRow } from "./types";

vi.mock("./api", () => ({
  getChapter: vi.fn(),
  fetchChapterAndWait: vi.fn(),
  listChapters: vi.fn()
}));
vi.mock("./ficSource", () => ({
  ficSourceAdapter: vi.fn(() => ({ chapterUrl: vi.fn(() => "https://example.test/chapter") }))
}));

import * as api from "./api";

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
  bookmarked: false,
  lastChapterNum: null,
  ffnUpdatedAt: null,
  source: "ffn"
};

const chapters: ChapterListItem[] = [{ chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null }];

describe("ensureChapterCached", () => {
  it("returns the cached chapter without fetching", async () => {
    const cachedContent: ChapterContent = { title: "Ch 2", content: "<p>hi</p>", isCached: true };
    vi.mocked(api.getChapter).mockResolvedValueOnce(cachedContent);

    const result = await ensureChapterCached(story, chapters, story.id, 2);

    expect(result).toEqual({ content: cachedContent, chapters, wasFetched: false });
    expect(api.fetchChapterAndWait).not.toHaveBeenCalled();
  });

  it("fetches, refreshes chapters, and returns the new content when not cached", async () => {
    const refreshedChapters: ChapterListItem[] = [{ chapterNum: 2, title: "Ch 2", isCached: true, sourceChapterId: null }];
    const fetchedContent: ChapterContent = { title: "Ch 2", content: "<p>fetched</p>", isCached: true };
    vi.mocked(api.getChapter).mockResolvedValueOnce(null).mockResolvedValueOnce(fetchedContent);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValueOnce(undefined);
    vi.mocked(api.listChapters).mockResolvedValueOnce(refreshedChapters);

    const result = await ensureChapterCached(story, chapters, story.id, 2);

    expect(api.fetchChapterAndWait).toHaveBeenCalledWith("ffn", "https://example.test/chapter", story.id, 2);
    expect(result).toEqual({ content: fetchedContent, chapters: refreshedChapters, wasFetched: true });
  });

  it("returns null content if the chapter still isn't persisted after a fetch", async () => {
    vi.mocked(api.getChapter).mockResolvedValueOnce(null).mockResolvedValueOnce(null);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValueOnce(undefined);
    vi.mocked(api.listChapters).mockResolvedValueOnce(chapters);

    const result = await ensureChapterCached(story, chapters, story.id, 2);

    expect(result.content).toBeNull();
    expect(result.wasFetched).toBe(true);
  });
});
