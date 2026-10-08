// The listing-filter vocabulary the Search bar, Bookmarks bar, SearchModal
// and StoryList render. Plain data: the `Record<Union, …>` types make a new
// `SortBy` / `StorySource` / `StoryStatus` member without a label a compile
// error instead of a silently missing <option> (the unions themselves mirror
// the Rust enums — see types.ts).

import type { SortBy, StorySource, StoryStatus } from "./types";

export const SORT_LABELS: Record<SortBy, string> = {
  favorites: "Favorites",
  fetched: "Recently fetched",
  words: "Word count"
};
export const SORT_OPTIONS = Object.entries(SORT_LABELS) as Array<[SortBy, string]>;

export const SOURCE_LABELS: Record<StorySource, string> = { ffn: "FFN", ao3: "AO3" };

export const STATUS_LABELS: Record<StoryStatus, string> = { complete: "Done", in_progress: "Ongoing" };

/// Statuses a *new* listing search can ask a Source for. FFN's browse URL
/// can't encode `in_progress` (SPEC.md §4), so its modal offers Done only;
/// the Search list's own status filter narrows locally and offers both.
export const SEARCH_STATUSES: Record<StorySource, StoryStatus[]> = {
  ffn: ["complete"],
  ao3: ["complete", "in_progress"]
};

export const WORD_COUNT_PRESETS = [
  { value: 40000, label: "40K+" },
  { value: 100000, label: "100K+" }
] as const;
