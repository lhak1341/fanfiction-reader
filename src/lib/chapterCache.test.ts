import { describe, expect, it, vi } from "vitest";
import { ensureChapterCached } from "./chapterCache";
import type { ChapterContent, ChapterListItem } from "./types";

vi.mock("./api", () => ({
  getChapter: vi.fn(),
  fetchChapterAndWait: vi.fn(),
  listChapters: vi.fn()
}));

import * as api from "./api";

const storyId = 1;
const chapters: ChapterListItem[] = [{ chapterNum: 2, title: "Ch 2", isCached: false, sourceChapterId: null }];

describe("ensureChapterCached", () => {
  it("returns the cached chapter without fetching", async () => {
    const cachedContent: ChapterContent = { title: "Ch 2", content: "<p>hi</p>", isCached: true };
    vi.mocked(api.getChapter).mockResolvedValueOnce(cachedContent);

    const result = await ensureChapterCached(storyId, 2);

    expect(result).toEqual({ content: cachedContent, chapters: null, wasFetched: false });
    expect(api.fetchChapterAndWait).not.toHaveBeenCalled();
  });

  it("fetches, refreshes chapters, and returns the new content when not cached", async () => {
    const refreshedChapters: ChapterListItem[] = [{ chapterNum: 2, title: "Ch 2", isCached: true, sourceChapterId: null }];
    const fetchedContent: ChapterContent = { title: "Ch 2", content: "<p>fetched</p>", isCached: true };
    vi.mocked(api.getChapter).mockResolvedValueOnce(null).mockResolvedValueOnce(fetchedContent);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValueOnce(undefined);
    vi.mocked(api.listChapters).mockResolvedValueOnce(refreshedChapters);

    const result = await ensureChapterCached(storyId, 2);

    expect(api.fetchChapterAndWait).toHaveBeenCalledWith(storyId, 2, false);
    expect(result).toEqual({ content: fetchedContent, chapters: refreshedChapters, wasFetched: true });
  });

  it("passes background through to the fetch", async () => {
    vi.mocked(api.getChapter).mockResolvedValueOnce(null).mockResolvedValueOnce(null);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValueOnce(undefined);
    vi.mocked(api.listChapters).mockResolvedValueOnce(chapters);

    await ensureChapterCached(storyId, 2, { background: true });

    expect(api.fetchChapterAndWait).toHaveBeenCalledWith(storyId, 2, true);
  });

  it("returns null content if the chapter still isn't persisted after a fetch", async () => {
    vi.mocked(api.getChapter).mockResolvedValueOnce(null).mockResolvedValueOnce(null);
    vi.mocked(api.fetchChapterAndWait).mockResolvedValueOnce(undefined);
    vi.mocked(api.listChapters).mockResolvedValueOnce(chapters);

    const result = await ensureChapterCached(storyId, 2);

    expect(result.content).toBeNull();
    expect(result.wasFetched).toBe(true);
  });
});
