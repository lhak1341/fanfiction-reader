// Mirrors the Rust structs Tauri serializes back to the frontend
// (queries.rs/db.rs/lib.rs, SPEC.md §5/§6) — field casing matches serde's
// `rename_all = "camelCase"` output, and interface names match their Rust
// struct name exactly. `src-tauri/tests/type_mirror.rs` (architecture
// review, 2026-07-29) fails `cargo test` if the two sides' field sets drift
// — see its own doc comment for exactly which structs it covers and why.

export type StoryStatus = "complete" | "in_progress";
// What a listing search asks for — a filter, not a story's status (Rust: `ListingStatus`, source.rs).
export type ListingStatus = StoryStatus | "any";
export type SortBy = "favorites" | "updated" | "words";
export type StorySource = "ffn" | "ao3";

export interface StoryRow {
  id: number;
  sourceStoryId: number;
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
  source: StorySource;
  /** How many of `chapterCount` chapters are already cached locally.
   * `=== chapterCount` means fully downloaded/offline-ready. */
  cachedChapterCount: number;
}

export interface StoryFilters {
  fandom?: string | null;
  /** `undefined`/`null` means both sources (SPEC.md §8: sources aren't
   * merged into one ranking, so sort-by-favorites only makes sense scoped
   * to one at a time). */
  source?: StorySource | null;
  search?: string | null;
  minWordCount?: number | null;
  status?: StoryStatus | null;
  sortBy?: SortBy;
  hideRead?: boolean;
  hideDismissed?: boolean;
  hideBookmarked?: boolean;
}

export interface ChapterListItem {
  chapterNum: number;
  title: string;
  isCached: boolean;
  /** AO3's opaque per-chapter id — needed to build a
   * `/works/{id}/chapters/{id}` fetch URL. `null` for FFN chapters. */
  sourceChapterId: number | null;
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

// Names below match their Rust struct one-for-one (lib.rs/queries.rs) on
// purpose — a drift-detecting test (queries.rs/db.rs/lib.rs's Serialize
// structs vs this file) pairs the two sides by identical name, no
// hand-maintained Rust-name-to-TS-name mapping to itself drift out of sync.

export interface ChapterFetched {
  storyId: number;
  chapterNum: number;
}

export interface BrowseFetched {
  fandom: string;
  count: number;
}

export interface FandomIndexEntry {
  category: string;
  fandom: string;
  ffnCount: number;
}

export interface FandomIndexFetched {
  category: string;
  count: number;
}
