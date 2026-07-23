// One seam for toggling a story's isRead/dismissed/bookmarked flag
// (architecture review candidate 2, 2026-07-23) — `+page.svelte` and
// ReaderScreen each need a different reconciliation strategy afterward
// (a full list reload vs. patching one held-onto StoryRow), so this stays a
// pure invoke-and-return-the-patched-row function rather than owning that
// policy itself; callers decide what to do with the result.

import * as api from "./api";
import type { StoryRow } from "./types";

export type StoryFlag = "isRead" | "dismissed" | "bookmarked";

const TOGGLE: Record<StoryFlag, (storyId: number, value: boolean) => Promise<void>> = {
  isRead: api.toggleRead,
  dismissed: api.toggleDismissed,
  bookmarked: api.toggleBookmarked
};

export async function toggleStoryFlag(story: StoryRow, flag: StoryFlag): Promise<StoryRow> {
  const value = !story[flag];
  await TOGGLE[flag](story.id, value);
  return { ...story, [flag]: value };
}
