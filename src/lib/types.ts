// Mirrors the Rust structs in app/src-tauri/src/queries.rs (SPEC.md §5/§6).
// Field casing matches serde's `rename_all = "camelCase"` output.

export type StoryStatus = "complete" | "in_progress";
export type SortBy = "favorites" | "updated" | "words";

export interface StoryRow {
  id: number;
  ffnStoryId: number;
  title: string;
  author: string;
  fandom: string;
  rating: string;
  genre: string | null;
  language: string | null;
  tags: string | null;
  status: StoryStatus;
  wordCount: number;
  favoriteCount: number;
  chapterCount: number;
  summary: string | null;
  isRead: boolean;
  dismissed: boolean;
  bookmarked: boolean;
  lastChapterNum: number | null;
  /** Raw "Updated:" text from FFN's metadata line, verbatim. `null` for
   * one-shots or stories imported before this field existed. */
  ffnUpdatedAt: string | null;
}

export interface StoryFilters {
  fandom?: string | null;
  search?: string | null;
  minWordCount?: number | null;
  status?: StoryStatus | null;
  sortBy?: SortBy;
  hideReadAndDismissed?: boolean;
  bookmarkedOnly?: boolean;
}

export interface ChapterListItem {
  chapterNum: number;
  title: string;
  isCached: boolean;
}

export interface ChapterContent {
  title: string;
  content: string;
  isCached: boolean;
}

// FFN's own fixed top-level categories — no scraping needed, these don't change.
export const FFN_CATEGORIES = [
  { value: "anime", label: "Anime/Manga" },
  { value: "book", label: "Books" },
  { value: "cartoon", label: "Cartoons" },
  { value: "comic", label: "Comics" },
  { value: "game", label: "Games" },
  { value: "misc", label: "Misc" },
  { value: "movie", label: "Movies" },
  { value: "play", label: "Plays" },
  { value: "tv", label: "TV" }
] as const;

export interface BrowseFetchedEvent {
  fandom: string;
  count: number;
}

export interface FandomIndexEntry {
  category: string;
  fandom: string;
  ffnCount: number;
}

export interface FandomIndexFetchedEvent {
  category: string;
  count: number;
}
