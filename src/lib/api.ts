// Typed wrappers over the Tauri commands in app/src-tauri/src/lib.rs.
// Keeps `invoke()` string command names in one place.

import { invoke } from "@tauri-apps/api/core";
import { awaitCorrelatedEvent, defineFetchEvent } from "./tauriEvents";
import { runPacedBestEffort } from "./pacedLoop";
import type {
  BrowseFetched,
  BrowseFetchError,
  ChapterContent,
  ChapterFetched,
  ChapterFetchError,
  ChapterListItem,
  FandomIndexEntry,
  FandomIndexFetched,
  FandomIndexFetchError,
  StoryFilters,
  StoryRow,
  StorySource,
  StoryStatus
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

export function setResumeChapter(storyId: number, chapterNum: number): Promise<void> {
  return invoke("set_resume_chapter", { storyId, chapterNum });
}

export function listChapters(storyId: number): Promise<ChapterListItem[]> {
  return invoke("list_chapters", { storyId });
}

export function getChapter(storyId: number, chapterNum: number): Promise<ChapterContent | null> {
  return invoke("get_chapter", { storyId, chapterNum });
}

export function getContinueReading(limit: number): Promise<StoryRow[]> {
  return invoke("get_continue_reading", { limit });
}

// ---- On-demand chapter fetch (SPEC.md §3/§4/§8) ----
// `fetch_chapter` resolves the story's source and the chapter's URL from the
// DB (`dispatch::chapter_fetch_url`) and delegates to `fetch_page` (FFN: opens
// a real webview, returns once the window exists, persists on page-load) or
// `fetch_ao3_page` (AO3: fetches and persists in one direct call, §8). Both
// emit the *same* `chapter-fetched`/`chapter-fetch-error` events, so callers
// await one of those either way, not the command's own promise.

const chapterEvents = defineFetchEvent<ChapterFetched, ChapterFetchError>("chapter-fetched", "chapter-fetch-error");

/// Fetches one chapter and resolves once it's actually persisted (or rejects
/// on a fetch error) — for callers that need to know when it's safe to read
/// the result back out of the DB. An unknown `storyId` rejects immediately,
/// before any fetch starts. Always fetches — check the cache first.
export function fetchChapterAndWait(storyId: number, chapterNum: number): Promise<void> {
  return awaitCorrelatedEvent<ChapterFetched, ChapterFetchError>(
    () => invoke("fetch_chapter", { storyId, chapterNum }),
    chapterEvents.on,
    chapterEvents.onError,
    (e) => e.storyId === storyId && e.chapterNum === chapterNum
  ).then(() => {});
}

/// Adds one story from a pasted URL (`add_story_by_url` validates it and picks
/// the source). Resolves with the first chapter event — deliberately no
/// `match` predicate, since a brand-new story has no `storyId` to correlate
/// on yet; callers must not run another chapter fetch concurrently (the
/// modal blocks submit during a bookmark refresh/download). A bad URL
/// rejects immediately, before any fetch starts.
export function addStoryByUrl(url: string): Promise<ChapterFetched> {
  return awaitCorrelatedEvent<ChapterFetched, ChapterFetchError>(
    () => invoke("add_story_by_url", { url }),
    chapterEvents.on,
    chapterEvents.onError
  );
}

// ---- FFN discovery (SPEC.md §6) ----
// Same fetch_page/report_fetch_result mechanism as chapters — a fetched
// browse page is parsed and imported, then a browse-fetched/-error event
// fires. `category` is required for a brand-new fandom; omit it (undefined)
// for "Load more" on an already-known one, where it's looked up server-side.

export function searchFfn(
  fandom: string,
  minWordCount: number,
  status: StoryStatus | "any",
  category?: string
): Promise<void> {
  return invoke("search_ffn", { category: category ?? null, fandom, minWordCount, status });
}

const browseEvents = defineFetchEvent<BrowseFetched, BrowseFetchError>("browse-fetched", "browse-fetch-error");

/// Awaitable version of `searchFfn`, matching `fetchChapterAndWait`'s shape.
/// Matched on `fandom` — without it, a concurrent "load more"/modal search
/// for a *different* fandom could resolve/reject this call with the wrong
/// result (architecture review candidate, 2026-07-29).
export function searchFfnAndWait(
  fandom: string,
  minWordCount: number,
  status: StoryStatus | "any",
  category?: string
): Promise<BrowseFetched> {
  return awaitCorrelatedEvent<BrowseFetched, BrowseFetchError>(
    () => searchFfn(fandom, minWordCount, status, category),
    browseEvents.on,
    browseEvents.onError,
    (e) => e.fandom === fandom
  );
}

// ---- AO3 discovery (SPEC.md §8) ----
// Same fetch_ao3_page mechanism as an AO3 chapter fetch, and the same
// browse-fetched/-error events as searchFfn — the direct `reqwest` call
// happens synchronously inside the Tauri command (§8), no separate
// window/callback round-trip like FFN needs.

/// `fandom` is force-applied to every imported row (see `fetchAo3Page`'s
/// `fandomOverride`) so it lands in the same fandom bucket as however this
/// fandom is already named on the FFN side, rather than AO3's own canonical
/// tag text splitting into a separate bucket.
export function searchAo3(tag: string, fandom: string, minWordCount: number, status: StoryStatus | "any"): Promise<void> {
  return invoke("search_ao3", { tag, fandom, minWordCount, status });
}

/// Awaitable version of `searchAo3`, matching `searchFfnAndWait`'s shape.
/// Matched on `fandom`, same reasoning as `searchFfnAndWait`.
export function searchAo3AndWait(
  tag: string,
  fandom: string,
  minWordCount: number,
  status: StoryStatus | "any"
): Promise<BrowseFetched> {
  return awaitCorrelatedEvent<BrowseFetched, BrowseFetchError>(
    () => searchAo3(tag, fandom, minWordCount, status),
    browseEvents.on,
    browseEvents.onError,
    (e) => e.fandom === fandom
  );
}

// ---- Fandom index (SPEC.md §6 — precise fandom picker) ----
// One page covers a whole category (FFN doesn't paginate the index), so
// this always replaces the cached list, no "load more" needed.

export function indexFfnCategory(category: string): Promise<void> {
  return invoke("index_ffn_category", { category });
}

export function listFandomIndex(category?: string): Promise<FandomIndexEntry[]> {
  return invoke("list_fandom_index", { category: category ?? null });
}

const fandomIndexEvents = defineFetchEvent<FandomIndexFetched, FandomIndexFetchError>(
  "fandom-index-fetched",
  "fandom-index-fetch-error"
);

/// Matched on `category`, same reasoning as `searchFfnAndWait`.
export function indexFfnCategoryAndWait(category: string): Promise<FandomIndexFetched> {
  return awaitCorrelatedEvent<FandomIndexFetched, FandomIndexFetchError>(
    () => indexFfnCategory(category),
    fandomIndexEvents.on,
    fandomIndexEvents.onError,
    (e) => e.category === category
  );
}

/// Indexes every category sequentially (see `runPacedBestEffort`, pacedLoop.ts,
/// for why this is paced). `onProgress` fires after each category (success or
/// failure) so the caller can show a running status.
export async function indexAllCategoriesAndWait(
  categories: readonly string[],
  onProgress?: (category: string, result: FandomIndexFetched | Error) => void
): Promise<void> {
  await runPacedBestEffort(categories, indexFfnCategoryAndWait, (_i, _total, category, result) =>
    onProgress?.(category, result)
  );
}
