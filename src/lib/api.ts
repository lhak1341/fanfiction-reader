// Typed wrappers over the Tauri commands in app/src-tauri/src/lib.rs.
// Keeps `invoke()` string command names in one place.

import { invoke } from "@tauri-apps/api/core";
import type {
  BrowseFetched,
  ChapterFetched,
  ChapterListItem,
  EnsuredChapter,
  FandomIndexEntry,
  FandomIndexFetched,
  ListingStatus,
  StoryFilters,
  StoryRow,
  StorySource,
} from "./types";

export function listStories(filters: StoryFilters): Promise<StoryRow[]> {
  return invoke("list_stories", { filters });
}

export function listBookmarks(filters: StoryFilters): Promise<StoryRow[]> {
  return invoke("list_bookmarks", { filters });
}

export function getStory(storyId: number): Promise<StoryRow | null> {
  return invoke("get_story", { storyId });
}

export function listFandoms(): Promise<string[]> {
  return invoke("list_fandoms");
}

/// Renames a fandom bucket across every story that has it — merges into an
/// existing fandom if `newFandom` already names one. Returns rows changed.
export function renameFandom(oldFandom: string, newFandom: string): Promise<number> {
  return invoke("rename_fandom", { oldFandom, newFandom });
}

export function listBookmarkedFandoms(): Promise<string[]> {
  return invoke("list_bookmarked_fandoms");
}

export function toggleRead(storyId: number, value: boolean): Promise<void> {
  return invoke("toggle_read", { storyId, value });
}

export function toggleDismissed(storyId: number, value: boolean): Promise<void> {
  return invoke("toggle_dismissed", { storyId, value });
}

export function toggleBookmarked(storyId: number, value: boolean): Promise<void> {
  return invoke("toggle_bookmarked", { storyId, value });
}

/// One resume point: the chapter and how far down it you are (0–100). Always
/// written as a pair, so a late save for an earlier chapter can't attach its
/// position to a newer one.
export function setResumeChapter(
  storyId: number,
  chapterNum: number,
  scrollPct: number
): Promise<void> {
  return invoke("set_resume_chapter", { storyId, chapterNum, scrollPct });
}

export function listChapters(storyId: number): Promise<ChapterListItem[]> {
  return invoke("list_chapters", { storyId });
}

export function getContinueReading(limit: number): Promise<StoryRow[]> {
  return invoke("get_continue_reading", { limit });
}

// ---- Fetching (SPEC.md §3/§4/§8, §6 discovery) ----
// Every fetch command resolves with its parsed-and-persisted result once the
// page has been fetched (FFN: webview round-trip, AO3: direct HTTP — both
// awaited inside the Rust command), or rejects: interrupted by a newer fetch,
// timed out, Cloudflare-blocked, or a parse failure. There are no result
// events to correlate (`fetch_session.rs`).

/// Fetches one chapter and resolves once it's actually persisted (or rejects
/// on a fetch error) — for callers that need to know when it's safe to read
/// the result back out of the DB. An unknown `storyId` rejects immediately,
/// before any fetch starts. Always fetches — check the cache first.
/// `background` marks bulk work (download, refresh, Reader prefetch): it never
/// interrupts a fetch the user is waiting on and rejects instead (`BUSY`,
/// fetch_session.rs). Sent explicitly — see src-tauri/CLAUDE.md.
export function fetchChapterAndWait(
  storyId: number,
  chapterNum: number,
  background = false
): Promise<void> {
  return invoke<ChapterFetched>("fetch_chapter", { storyId, chapterNum, background }).then(
    () => {}
  );
}

/// Makes chapter `chapterNum` cached and returns it — the stored body on a
/// cache hit, else a fetch (same `background` rules as `fetchChapterAndWait`)
/// plus the refreshed chapter list and story chapter count. `chapterCount` and
/// `chapters` are `null` on a hit: nothing changed, keep what you hold.
export function ensureChapter(
  storyId: number,
  chapterNum: number,
  background = false
): Promise<EnsuredChapter> {
  return invoke<EnsuredChapter>("ensure_chapter", { storyId, chapterNum, background });
}

/// Adds one story from a pasted URL (`add_story_by_url` validates it and picks
/// the source) and bookmarks it — a pasted URL is intent to keep the story. A
/// bad URL rejects immediately, before any fetch starts. A
/// foreground fetch: it supersedes a running bookmark refresh/download item
/// (which fails and is retried or reported) rather than waiting for it, and
/// like any FFN fetch a newer foreground one can interrupt it.
export function addStoryByUrl(url: string): Promise<ChapterFetched> {
  return invoke("add_story_by_url", { url });
}

// ---- FFN discovery (SPEC.md §6) ----
// Same fetch_page mechanism as chapters — a fetched browse page is parsed and
// imported before the call resolves. `category` is required for a brand-new
// fandom; omit it (undefined) for "Load more" on an already-known one, where
// it's looked up server-side.

export function searchFfnAndWait(
  fandom: string,
  minWordCount: number,
  status: ListingStatus,
  category?: string
): Promise<BrowseFetched> {
  return invoke("search_ffn", { category: category ?? null, fandom, minWordCount, status });
}

// ---- AO3 discovery (SPEC.md §8) ----
// The direct `reqwest` call happens inside the Tauri command (§8), no
// separate window round-trip like FFN needs.

/// `fandom` is force-applied to every imported row (see `fetchAo3Page`'s
/// `fandomOverride`) so it lands in the same fandom bucket as however this
/// fandom is already named on the FFN side, rather than AO3's own canonical
/// tag text splitting into a separate bucket.
export function searchAo3AndWait(
  tag: string,
  fandom: string,
  minWordCount: number,
  status: ListingStatus
): Promise<BrowseFetched> {
  return invoke("search_ao3", { tag, fandom, minWordCount, status });
}

// ---- Fandom index (SPEC.md §6 — precise fandom picker) ----
// One page covers a whole category (FFN doesn't paginate the index), so
// this always replaces the cached list, no "load more" needed.

export function listFandomIndex(category?: string): Promise<FandomIndexEntry[]> {
  return invoke("list_fandom_index", { category: category ?? null });
}

export function indexFfnCategoryAndWait(category: string): Promise<FandomIndexFetched> {
  return invoke("index_ffn_category", { category });
}
