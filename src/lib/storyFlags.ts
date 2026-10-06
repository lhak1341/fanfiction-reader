// One seam for toggling a story's isRead/dismissed/bookmarked flag
// (architecture review candidate 2, 2026-07-23) — `+page.svelte` and
// ReaderScreen each need a different reconciliation strategy afterward
// (a full list reload vs. patching one held-onto StoryRow), so this stays a
// pure invoke-and-return-the-patched-row function rather than owning that
// policy itself; callers decide what to do with the result.

// Architecture review candidate 3 (2026-10-07), rejected: collapsing the
// three `toggle_*` commands / `api.toggleX` wrappers / `queries::set_*`
// one-liners into one `set_story_flag(id, flag, value)` would delete ~6
// shallow pieces, but the shared logic (`set_flag`, `toggleStoryFlag`) is
// already generic, and it would add a Rust-enum/TS-string wire contract, touch
// the command-registration guard and the debug page's raw `invoke`, and
// rewrite ~20 test lines that mock the per-flag `api` names. Reopen if a
// fourth flag is added.

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
