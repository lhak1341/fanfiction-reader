# Longform

A local-first reader for fanfiction imported from multiple sources (FanFiction.net, Archive of Our Own).

## Language

**Source**:
One of the two origin sites a Story can be imported from — FanFiction.net (`ffn`) or Archive of Our Own (`ao3`). Each has its own markup, tagging vocabulary, and fetch mechanics. In Rust it is the `StorySource` enum (`source.rs`) — the one value for a story's, a filter's and a pasted URL's source; in TS, the `StorySource` union (`types.ts`).

**Fandom**:
The canonical grouping a Story belongs to (e.g. "Naruto", "Harry Potter"), stored as `stories.fandom` — one label per Story, not per Source. The same fandom can be searched and imported from either Source; sources don't always agree on the fandom's exact label, which is why import force-applies one chosen label per search rather than trusting each Source's own text.

There is deliberately no cross-Source fandom-identity table (e.g. mapping a canonical fandom to each Source's own tag/label). What exists instead, by design, not as gaps to close: `rename_fandom` (`queries.rs`) is a manual one-time repair for buckets that ended up wrong before the force-apply above existed, or for AO3 crossover works whose `fandom` is a comma-joined multi-fandom string — it touches `stories.fandom` only, since `browse_state.fandom` is FFN's own fandom name (it builds the browse URL), so renaming a real FFN fandom to a custom label deliberately ends its FFN "Load more" rather than pointing it at a fandom FFN doesn't have; `fandom.ts`'s AO3 tag map is a client-side, per-user preference (typed once, reused after, and moved along by a rename via `renameAo3Tag`) because AO3 has no queryable fandom→tag index (`robots.txt` blocks `/autocomplete/`) and this isn't fetched data; `lastSearchSource` (`+page.svelte`) is a session-scoped guess for which Source "Load more" should continue when the fandom alone doesn't say. Revisit only if a concrete new pain point (not just tidiness) shows up.

**FicSource**:
The seam unifying the two Sources' parsing and fetch-dispatch operations — resolving a fetched page to either a chapter or a listing, and parsing/persisting either shape. Two parallel implementations: `Ffn`/`Ao3` (`src-tauri/src/source.rs`) on the backend, `ffnAdapter`/`ao3Adapter` (`src/lib/ficSource.ts`) on the frontend for search-continuation only. Chapter-URL-building lives solely in Rust (`FicSource::chapter_url`, reached through the `fetch_chapter` command): the frontend sends `(storyId, chapterNum)` and never builds a source URL. Deliberately narrow: it does *not* cover DB upsert (`upsert_story`/`upsert_ao3_story` stay separate — the two sources' metadata shapes don't overlap enough to share a schema-facing signature), FFN's category-index fetch (no AO3 equivalent), or `SearchModal.svelte`'s input UI (a genuine UX difference, not duplication).
_Avoid_: adapter (for the whole concept — "adapter" refers to one of the two `Ffn`/`Ao3` (or `ffnAdapter`/`ao3Adapter`) impls _behind_ FicSource, not the seam itself), source handler.

**Fetch session**:
The one in-flight FanFiction.net webview fetch and its lifecycle — start, supersede, deliver, time out (`src-tauri/src/fetch_session.rs`). A new fetch rejects the one it replaces ("newest wins"); a fetch the webview never reports back from rejects after 30 s; a reported page is matched to the pending fetch by what it *is* (same chapter, listing page or category index), not by URL string, so FFN's slug redirect and non-ASCII fandoms still match. It hands the rendered page to the command waiting on it and never persists — parsing and the DB write happen in that command, the same shape as AO3's direct fetch, which does not use it. Fetches carry a priority: a background fetch (Download-all, refresh, Reader prefetch) never supersedes a pending foreground one and fails fast with `BUSY`; equal priorities keep "newest wins". A full scheduler (queueing, a minimum gap between navigations) is still not built.
_Avoid_: pending fetch, fetch lock (the old names for the pre-session bookkeeping).

**Story flag**:
One of a Story's three independent, manually-toggled per-row booleans — `isRead`, `dismissed` (not-for-me), `bookmarked` — stored on `read_state`. Toggling one flag is `storyFlags.ts`'s pure `toggleStoryFlag(story, flag)`, which persists via `api.ts` and returns the patched row; it deliberately has no opinion on what a caller does afterward, since `+page.svelte` (refresh both Search and Bookmarks lists) and `ReaderScreen` (patch its one held-onto `StoryRow`) need different reconciliation strategies. `storyFlagActions.ts` is the composition root's own strategy — `toggleReadAndRefresh`/`toggleDismissedAndRefresh`/`toggleBookmarkedAndRefresh`, each wrapping the toggle in error-handling and a caller-supplied `refreshAll`.

**Cached chapter**:
A chapter whose body is stored locally (`chapters.content <> ''`). A TOC stub — registered with every fetch of the story so titles stay browsable — has `content = ''` and is *not* cached. A fetch that parses to empty content is a persist error, never a stored empty row, so "fetched" and "cached" mean the same thing. `ensure_chapter` returns the cached chapter or fetches it; `fetch_chapter` always fetches (the debug page's bulk prefetch relies on that).
_Avoid_: downloaded, fetched (for the stored state).
