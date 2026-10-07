import * as api from "./api";
import type { ChapterContent, ChapterListItem } from "./types";

export interface EnsureChapterCachedResult {
  content: ChapterContent | null;
  chapters: ChapterListItem[] | null;
  wasFetched: boolean;
}

// Ensures chapter `num` of story `storyId` is persisted locally, fetching it
// if it isn't yet (the backend resolves the source and URL). A fetch refreshes
// and returns the chapter list, so callers can pick up accurate isCached flags
// (e.g. Reader.svelte's chapter nav/drawer) without a separate reload;
// `chapters` is `null` on a cache hit — nothing changed, so the caller keeps
// the list it has.
//
// `background` is for bulk callers (download, Reader prefetch): their fetch
// yields to a foreground one instead of interrupting it (see api.ts).
export async function ensureChapterCached(
  storyId: number,
  num: number,
  options: { background?: boolean } = {}
): Promise<EnsureChapterCachedResult> {
  const cached = await api.getChapter(storyId, num);
  if (cached?.isCached) {
    return { content: cached, chapters: null, wasFetched: false };
  }
  await api.fetchChapterAndWait(storyId, num, options.background ?? false);
  const refreshedChapters = await api.listChapters(storyId);
  const content = await api.getChapter(storyId, num);
  return { content: content ?? null, chapters: refreshedChapters, wasFetched: true };
}
