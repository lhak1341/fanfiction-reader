import * as api from "./api";
import { ficSourceAdapter } from "./ficSource";
import type { ChapterContent, ChapterListItem, StoryRow } from "./types";

export interface EnsureChapterCachedResult {
  content: ChapterContent | null;
  chapters: ChapterListItem[];
  wasFetched: boolean;
}

// Ensures chapter `num` of `story` is persisted locally, fetching it through
// the FicSource seam if it isn't yet. `chapters` only needs to be current
// enough to resolve `sourceChapterId` for the fetch URL (SPEC.md §8, AO3
// chapters beyond the first need it) — a fetch refreshes and returns it, so
// callers can pick up accurate isCached flags (e.g. Reader.svelte's chapter
// nav/drawer) without a separate reload.
export async function ensureChapterCached(
  story: StoryRow,
  chapters: ChapterListItem[],
  storyId: number,
  num: number
): Promise<EnsureChapterCachedResult> {
  const cached = await api.getChapter(storyId, num);
  if (cached?.isCached) {
    return { content: cached, chapters, wasFetched: false };
  }
  const sourceChapterId = chapters.find((c) => c.chapterNum === num)?.sourceChapterId;
  const url = ficSourceAdapter(story.source).chapterUrl(story, num, sourceChapterId);
  await api.fetchChapterAndWait(story.source, url, storyId, num);
  const refreshedChapters = await api.listChapters(storyId);
  const content = await api.getChapter(storyId, num);
  return { content: content ?? null, chapters: refreshedChapters, wasFetched: true };
}
