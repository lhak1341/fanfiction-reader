// The composition root's story-flag policy (architecture review candidate 1,
// 2026-07-24) — +page.svelte previously had three near-identical, untested
// handlers wrapping storyFlags.ts's pure toggleStoryFlag in try/catch and a
// cross-screen refresh. Collapsed into the same shape ReaderScreen.toggle
// already uses: one private implementation, three named wrappers. Doesn't
// reopen architecture review candidate 2 — toggleStoryFlag itself stays pure;
// this only gives the composition root's own reconciliation strategy
// (refresh both lists, report errors) a tested home. refreshAll is passed in
// rather than duplicated here since +page.svelte also uses it standalone for
// <Reader onChanged={refreshAll}>.

import type { ErrorBanner } from "./errorBanner.svelte";
import { toggleStoryFlag, type StoryFlag } from "./storyFlags";
import type { StoryRow } from "./types";

const MESSAGES: Record<StoryFlag, string> = {
  isRead: "Could not update read state",
  dismissed: "Could not update dismissed state",
  bookmarked: "Could not update bookmark",
};

async function toggleAndRefresh(
  story: StoryRow,
  flag: StoryFlag,
  banner: ErrorBanner,
  refreshAll: () => Promise<void>
): Promise<void> {
  await banner.run("flag", MESSAGES[flag], async () => {
    await toggleStoryFlag(story, flag);
    await refreshAll();
  });
}

export function toggleReadAndRefresh(
  story: StoryRow,
  banner: ErrorBanner,
  refreshAll: () => Promise<void>
) {
  return toggleAndRefresh(story, "isRead", banner, refreshAll);
}

export function toggleDismissedAndRefresh(
  story: StoryRow,
  banner: ErrorBanner,
  refreshAll: () => Promise<void>
) {
  return toggleAndRefresh(story, "dismissed", banner, refreshAll);
}

export function toggleBookmarkedAndRefresh(
  story: StoryRow,
  banner: ErrorBanner,
  refreshAll: () => Promise<void>
) {
  return toggleAndRefresh(story, "bookmarked", banner, refreshAll);
}
