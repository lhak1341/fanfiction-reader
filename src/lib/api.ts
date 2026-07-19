// Typed wrappers over the Tauri commands in app/src-tauri/src/lib.rs.
// Keeps `invoke()` string command names in one place.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BrowseFetchedEvent,
  ChapterContent,
  ChapterListItem,
  FandomIndexEntry,
  FandomIndexFetchedEvent,
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
// `fetch_page` (FFN) opens a real webview and returns as soon as the window
// is created — parsing/persisting happens later, on page-load. `fetch_ao3_page`
// (AO3) fetches and persists in one direct call instead (no webview needed,
// §8) but emits the *same* `chapter-fetched`/`chapter-fetch-error` events, so
// callers await one of those either way, not the fetch call's own promise.

export function fetchPage(url: string): Promise<void> {
  return invoke("fetch_page", { url });
}

/// `fandomOverride`, when given, force-applies that label to every row of a
/// tag-listing fetch (ignored for a chapter fetch) — see `searchAo3`.
export function fetchAo3Page(url: string, fandomOverride?: string): Promise<void> {
  return invoke("fetch_ao3_page", { url, fandomOverride: fandomOverride ?? null });
}

export interface ChapterFetchedEvent {
  storyId: number;
  chapterNum: number;
}

export function onChapterFetched(handler: (e: ChapterFetchedEvent) => void): Promise<UnlistenFn> {
  return listen<ChapterFetchedEvent>("chapter-fetched", (e) => handler(e.payload));
}

export function onChapterFetchError(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("chapter-fetch-error", (e) => handler(e.payload));
}

/// Fetches one chapter and resolves once it's actually persisted (or rejects
/// on a fetch error) — the awaitable version of `fetchPage`/`fetchAo3Page`,
/// for callers that need to know when it's safe to read the result back out
/// of the DB. `source` picks which Tauri command actually does the fetch;
/// both emit the same events so the waiting logic here is source-agnostic.
export function fetchChapterAndWait(source: StorySource, url: string, storyId: number, chapterNum: number): Promise<void> {
  return new Promise((resolve, reject) => {
    let unlistenOk: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    const cleanup = () => {
      unlistenOk?.();
      unlistenErr?.();
    };
    onChapterFetched((e) => {
      if (e.storyId === storyId && e.chapterNum === chapterNum) {
        cleanup();
        resolve();
      }
    }).then((fn) => (unlistenOk = fn));
    onChapterFetchError((msg) => {
      cleanup();
      reject(new Error(msg));
    }).then((fn) => (unlistenErr = fn));
    const fetchFn = source === "ao3" ? fetchAo3Page : fetchPage;
    fetchFn(url).catch((e) => {
      cleanup();
      reject(e);
    });
  });
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

export function onBrowseFetched(handler: (e: BrowseFetchedEvent) => void): Promise<UnlistenFn> {
  return listen<BrowseFetchedEvent>("browse-fetched", (e) => handler(e.payload));
}

export function onBrowseFetchError(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("browse-fetch-error", (e) => handler(e.payload));
}

/// Awaitable version of `searchFfn`, matching `fetchChapterAndWait`'s shape.
export function searchFfnAndWait(
  fandom: string,
  minWordCount: number,
  status: StoryStatus | "any",
  category?: string
): Promise<BrowseFetchedEvent> {
  return new Promise((resolve, reject) => {
    let unlistenOk: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    const cleanup = () => {
      unlistenOk?.();
      unlistenErr?.();
    };
    onBrowseFetched((e) => {
      cleanup();
      resolve(e);
    }).then((fn) => (unlistenOk = fn));
    onBrowseFetchError((msg) => {
      cleanup();
      reject(new Error(msg));
    }).then((fn) => (unlistenErr = fn));
    searchFfn(fandom, minWordCount, status, category).catch((e) => {
      cleanup();
      reject(e);
    });
  });
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
export function searchAo3AndWait(
  tag: string,
  fandom: string,
  minWordCount: number,
  status: StoryStatus | "any"
): Promise<BrowseFetchedEvent> {
  return new Promise((resolve, reject) => {
    let unlistenOk: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    const cleanup = () => {
      unlistenOk?.();
      unlistenErr?.();
    };
    onBrowseFetched((e) => {
      cleanup();
      resolve(e);
    }).then((fn) => (unlistenOk = fn));
    onBrowseFetchError((msg) => {
      cleanup();
      reject(new Error(msg));
    }).then((fn) => (unlistenErr = fn));
    searchAo3(tag, fandom, minWordCount, status).catch((e) => {
      cleanup();
      reject(e);
    });
  });
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

export function onFandomIndexFetched(handler: (e: FandomIndexFetchedEvent) => void): Promise<UnlistenFn> {
  return listen<FandomIndexFetchedEvent>("fandom-index-fetched", (e) => handler(e.payload));
}

export function onFandomIndexFetchError(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("fandom-index-fetch-error", (e) => handler(e.payload));
}

export function indexFfnCategoryAndWait(category: string): Promise<FandomIndexFetchedEvent> {
  return new Promise((resolve, reject) => {
    let unlistenOk: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    const cleanup = () => {
      unlistenOk?.();
      unlistenErr?.();
    };
    onFandomIndexFetched((e) => {
      cleanup();
      resolve(e);
    }).then((fn) => (unlistenOk = fn));
    onFandomIndexFetchError((msg) => {
      cleanup();
      reject(new Error(msg));
    }).then((fn) => (unlistenErr = fn));
    indexFfnCategory(category).catch((e) => {
      cleanup();
      reject(e);
    });
  });
}

/// Indexes every category sequentially, one webview fetch at a time with a
/// gap between them — same reasoning as chapter prefetching: several fetches
/// back-to-back reads as bot traffic to Cloudflare in a way a single fetch
/// doesn't. `onProgress` fires after each category (success or failure) so
/// the caller can show a running status.
export async function indexAllCategoriesAndWait(
  categories: readonly string[],
  onProgress?: (category: string, result: FandomIndexFetchedEvent | Error) => void
): Promise<void> {
  for (let i = 0; i < categories.length; i++) {
    const category = categories[i];
    try {
      const result = await indexFfnCategoryAndWait(category);
      onProgress?.(category, result);
    } catch (e) {
      onProgress?.(category, e instanceof Error ? e : new Error(String(e)));
    }
    if (i < categories.length - 1) {
      await new Promise((r) => setTimeout(r, 1500 + Math.random() * 1500));
    }
  }
}
