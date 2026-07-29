# Longform (FanFiction Reader) — App Spec

A solo, local-only desktop app for reading fanfiction.net, built to fix the two things the official site and app get wrong: filtering/read-state management, and finding new fics worth reading. This doc reflects the app as actually built — see `docs/wayfinder/map.md` and its closed tickets for the original decision trail, and this doc's own revision history (git) for what changed during implementation.

## 1. Scope

**In scope**: a macOS desktop app that scrapes fanfiction.net search/browse/discovery pages and story chapters, renders chapters in-app, caches them locally for offline reading, and layers filters, personal state (read/dismissed/bookmarked), and reading preferences on top that FFN's own site/app don't offer.

**Out of scope** (v1):
- Distribution or sharing to other users — this is a personal tool, not a product. Keeps legal exposure from ToS/Cloudflare scraping to personal-use scale.
- Platforms beyond macOS (Windows/Linux/mobile).
- Writing, reviewing, or uploading to FFN — reading only.
- Automatic mark-as-read — manual toggle only.
- Saved filter presets — schema has a `filter_presets` table but no UI/commands were built on top of it. The actual bottleneck (finding new stories) is solved by FFN discovery (§6) remembering pagination per search; the local filter bar is low-friction enough that presets weren't worth the complexity. Left as unused, harmless schema.

## 2. Architecture

- **Shell**: Tauri, macOS-first.
- **Frontend**: Svelte + Bun, SvelteKit with `adapter-static`. Two routes: `/` (the real app) and `/debug` (dev tools — fetch spike, DB inspector, chapter prefetch; not part of the shipped UX).
- **Backend (Rust) crates**:
  - `tauri` — app shell
  - `scraper` — HTML parsing, paired with the FanFicFare-ported extraction logic (§4)
  - `rusqlite` (bundled) — local SQLite storage
  - `serde` / `serde_json` — data passing between the Rust backend and the Svelte frontend via Tauri commands
  - `url` — URL parsing/construction (story/chapter ids, browse-page query params)
  - `reqwest` — sat unused through v1 (the webview strategy, §3, was sufficient for FFN); now the actual AO3 fetch path (§8) — a bare client works there, no webview needed.
- **lucide-svelte** — icon set used throughout instead of unicode glyphs.

**Maintenance note — adding a new Tauri command**: registering it in `invoke_handler!` (`lib.rs`) is not enough on its own; Tauri v2's permission system also requires it added to `commands.allow` in some `permissions/*.toml` file (referenced by the right capability for the window it needs), or the frontend gets a runtime "not allowed" error with no compile-time warning. Confirmed the hard way (2026-07-19): `search_ao3`/`fetch_ao3_page` compiled and ran fine but were rejected at `invoke()` time until added there. Relatedly, editing `permissions/*.toml` (or `capabilities/*.json`) does **not** reliably trigger a rebuild — Cargo's build-script fingerprinting is content-based, not mtime-based, so even `touch build.rs` doesn't force `tauri_build::build()` to rerun; only an actual content change to `build.rs` (or a full `cargo clean -p app`) does. **Guarded by a test since the architecture review (2026-07-19)**: `src-tauri/tests/command_registration.rs` asserts `invoke_handler!`'s command set equals the union of `commands.allow` across every `permissions/*.toml` file — a forgotten entry now fails `cargo test`, not just the running app.

**Other architecture review deepenings (2026-07-19, no behavior change)**: `src/lib/tauriEvents.ts`'s `awaitCorrelatedEvent` replaces four near-identical invoke-and-wait-for-event implementations in `api.ts`; `src/lib/chapterCache.ts`'s `ensureChapterCached` replaces a duplicated cache-check/fetch/refresh sequence inlined twice in `Reader.svelte`; `db.rs`'s `migrate()` iterates a `const MIGRATIONS` list instead of a hardcoded sequence of calls (no `schema_version` table — every migration here is already independently self-guarding, safe to re-run every launch); `lib.rs`'s `FetchOutcome` split into a 2-variant type (`Chapter`/`Browse`, used by the generic dispatch and AO3's path) plus a 3-variant `FfnFetchOutcome` (adds `FandomIndex`, FFN-only), so AO3's dispatch is exhaustive with no `unreachable!()`. First frontend test coverage added this session too: `vitest` (`bun run test`) with unit tests for `chapterCache.ts`, `typography.ts`, `format.ts`, `fandom.ts`.

## 3. Fetching & Cloudflare strategy

fanfiction.net sits behind Cloudflare bot protection.

**Decision**: fetch through **Tauri's own WebView** (WKWebView on macOS), not a bare HTTP client — confirmed necessary: bare `reqwest`/`curl` are blocked by TLS/connection fingerprinting even with a copied `cf_clearance` cookie, and the mobile subdomain (`m.fanfiction.net`) gets no lighter protection than `www.` (FanFicFare's own adapter rewrites `m.` URLs to `www.` before parsing, confirming this). All scraping targets `www.fanfiction.net` markup.

**How it actually works** (`fetch_page` command in `lib.rs`): opens a Tauri webview window (label `fetch-page`) pointed at the target URL. **Hidden by default** (`.visible(false)`) — a real window flashing on screen for every chapter fetch/prefetch was disruptive; the tradeoff is that if fanfiction.net ever serves an interactive Cloudflare challenge instead of passing straight through (rare once `cf_clearance` is established), there's no visible window to solve it in and the fetch just times out — flip `.visible(false)` back to `true` temporarily to debug that case. An **initialization script** (runs before the page's own JS, on every navigation) listens for the page's `DOMContentLoaded` event — not `load`, and not Tauri's own `on_page_load(Finished)` delegate event, both of which proved unreliable in testing: pages with slow/hanging subresources (ad/tracking iframes, long-polling) never fire `load`, and Tauri's navigation-delegate `Finished` event didn't fire at all on some pages (browse/category listings in particular) despite the content rendering correctly on screen. `DOMContentLoaded` plus a 4-second fallback timeout proved reliable. On fire, the script invokes `report_fetch_result` with the page's HTML, URL, and title; that command dispatches on **URL shape** to one of three parsers (§4) and persists the result, then emits a success/error event. The window is left open (still hidden), not closed — closing it forced every subsequent fetch back through `WebviewWindowBuilder::build()`, and on macOS building a *new* NSWindow steals OS focus regardless of `.focused(false)` (that flag only controls focus once the app is already frontmost), which was yanking focus back on every chapter of a bulk download even while `.visible(false)`. A single `fetch-page` window is instead reused per fetch via `navigate()` (which doesn't create a window, so doesn't activate anything) — the window is only ever *built* once, on the app's first fetch. Reuse means only one fetch is ever in flight, which also means rapid repeated fetches (e.g. bulk chapter prefetching, or the Reader's own chapter prefetch below) can look bot-like to Cloudflare; the app spaces out any multi-fetch operation by 1.5–3s between requests. A `FetchLock` (tokio `Mutex`, holding the currently-pending fetch's URL rather than just a bare guard) serializes access to the shared window and detects when a `navigate()` is about to interrupt a still-loading one — that interrupted fetch's own `DOMContentLoaded`/timeout listeners die with the navigation, so it would otherwise never call `report_fetch_result` and its awaiting caller would hang forever, silently. `fetch_page` classifies the interrupted URL (`classify_fetch_target`, same logic that targets a normal fetch failure at the right typed error event, architecture review 2026-07-29) and emits an "interrupted by a newer fetch" error for it immediately, before navigating — so an interrupted caller gets a real rejection instead of a silent hang.

**Blocked-page detection** is scoped to the `<title>` only (`"just a moment"`, `"attention required"`, `"checking your browser"`), plus a full-page check for the `cf-browser-verification` CSS class. Checking the *whole page text* for these phrases was tried first and produced false positives — ordinary story prose can innocently contain the phrase "just a moment."

## 4. Scraping

Three page types are parsed, all in `src-tauri/src/ffn.rs`. Two of them (chapter fetch, browse listing) dispatch through the `FicSource` trait (`src-tauri/src/source.rs`) shared with AO3 — one generic `try_persist_fetch::<S: FicSource>` (`lib.rs`) replaces what used to be two near-identical ~100-line functions (architecture review, 2026-07-19). The third (category index) has no AO3 equivalent and stays FFN-specific, dispatched directly in `lib.rs` outside the trait. The frontend has a matching seam, `src/lib/ficSource.ts` (`ffnAdapter`/`ao3Adapter`), covering chapter-URL-building and search-continuation — see `CONTEXT.md`'s `FicSource` entry for the full scope/non-scope of both.

| URL shape | Example | Parser |
|---|---|---|
| `/s/{id}/{chapter}/...` | `/s/8067258/1/Worth-Dying-For` | `parse_story_page` + `parse_chapter_content` |
| `/{category}/{fandom}/...` | `/anime/Naruto/?srt=4&len=100&p=1` | `parse_browse_page` |
| `/{category}/` | `/anime/` | `parse_fandom_index` |

Selectors below are real, verified against live-fetched markup (not guessed from FanFicFare's Python source, though the field mapping was ported from its `adapter_fanfictionnet.py`).

**Fixture tests (2026-07-19)**: all six parse functions across both sources (`parse_story_page`/`parse_chapter_content`/`parse_browse_page`/`parse_fandom_index` here, `parse_work_page`/`parse_listing_page`/`parse_chapter_content` in `ao3.rs`) now have unit tests against real captured markup in `src-tauri/tests/fixtures/*.html`, asserting concrete field values rather than just "doesn't error" — this is what caught the `extract_trailing_tags` bug above. FFN fixtures can only be captured via the app's own webview (bare fetches are Cloudflare-blocked, confirmed live during this session even with a real desktop UA — see §3); AO3 fixtures can be fetched directly (`python3 urllib` + a real UA header).

### Story page (`parse_story_page`)

- Title: `b.xcontrast_txt` inside `#profile_top`.
- Author: `a.xcontrast_txt[href^="/u/"]` inside `#profile_top`.
- Summary: the one direct-child `div.xcontrast_txt` of `#profile_top`.
- **Fandom**: NOT in the metadata line — it's the last link in the `#pre_story_links` breadcrumb (e.g. "Anime/Manga › Naruto").
- Metadata line (`span.xgray` inside `#profile_top`): `Rated: Fiction T - English - Adventure/Sci-Fi - Naruto U. - Chapters: 22 - Words: 114,414 - Reviews: 395 - Favs: 779 - Follows: 835 - Updated: ... - Published: ... - id: 8067258`. Rating/language/genre read positionally between "Rated:" and the first keyed label; keyed fields (`Words`, `Favs`, `Chapters`) extracted by label; character/pairing tags are whatever's left after genre, before the keyed section.
- Status: "Complete" detected as a substring of the metadata line, not a keyed field.
- Chapter list: `select[name="chapter"] option` — value is chapter number, text is `"N. Title"` (number prefix stripped).
- Chapter content: `#storytext` inner HTML (preserves paragraphs/formatting for the reader).

### Browse/category listing page (`parse_browse_page`)

One `div.z-list` per story. Title+id: `a.stitle[href]` (href's 2nd path segment is `ffn_story_id`). Author: `a[href^="/u/"]`. Summary: direct text nodes of the row's `div.z-indent`, excluding its nested metadata div. Metadata line (`div.z-padtop2`) has the **same fields as the story page but different tag placement** — character/pairing tags sit *after* "Published:", not after genre — and "Rated:" has no `<a>` wrapper or "Fiction" prefix here. Fandom isn't repeated per row; it's derived once from the URL path (`fandom_from_browse_url`/`parse_browse_url`) and applied to every parsed entry, since the page has no element naming it anywhere.

`chapter_count` comes straight from the listing's own "Chapters: N" field — no per-chapter titles are known yet for a newly-discovered story; those populate lazily the first time each chapter is actually opened, same as any story.

**Bug found and fixed (architecture review candidate 5, 2026-07-19)**: `extract_trailing_tags` didn't strip the trailing `"- Complete"` status marker FFN appends after the character/pairing tags on a *completed* story's row — it leaked straight into `tags` (e.g. `"Edward E., Roy M. - Complete"` instead of `"Edward E., Roy M."`). In-progress rows were never affected (no such marker to leak). Caught by a fixture test against real captured markup (`ffn_browse_listing.html`), fixed, regression-tested. 141 rows in the already-imported DB carry the stale value — not auto-repaired.

### Category fandom index (`parse_fandom_index`)

`#list_output` contains one `<div>` per fandom: `<a href="/anime/Naruto/" title="Naruto">Naruto</a> <span class="gray">(444K)</span>`. Already sorted by count descending. Counts are abbreviated (`444K`, `85.3K`, or plain `523`/`1` under 1,000) — parsed via `parse_ffn_count`. Only entries with **≥1,000 fics** are kept (locked scope decision — the long tail below that isn't worth surfacing in the picker); since the list is sorted, parsing stops at the first entry under the threshold rather than scanning all of it (a category can have 2,000+ total fandoms).

### FFN discovery URL construction (`build_browse_url`)

Confirmed against a real filtered FFN URL during development (`?srt=4&r=10&len=100&s=2`):
- `srt=4` = sort by Favorites. Always used for imports regardless of the app's own display sort — FFN has **no native sort-by-word-count**, so "sort by words" only ever exists as a local re-sort after import; Favorites is the best available relevance signal for which stories to pull first out of a potentially much larger matching set.
- `r=10` = no rating filter (include Mature-rated stories). Always sent — **this was documented here but missing from the actual `build_browse_url` implementation until it was caught by a live diff against fanfiction.net** (some of a fandom's most-favorited stories are M-rated and were silently absent from every import made before the fix). Without it, FFN's browse listing defaults to excluding M-rated content entirely.
- `len` = minimum word count **in thousands** (`len=100` → 100K+), omitted for "any length."
- `s=2` = Complete (the only status code confirmed; "in_progress"/"any" are left server-side-unfiltered and narrowed locally instead of risking a wrong guessed code).
- `p` = page number.

**Explicitly excluded from the port**: FanFicFare's other-site adapters, epub/mobi/output-format code, config/`defaults.ini` system, calibre-plugin/GUI, cover-image handling, author-page/series-listing scraping.

**Maintenance note**: all three parsers are coupled to fanfiction.net's current markup. If FFN changes it, parsing breaks silently (wrong/missing fields) or loudly (extraction errors, surfaced as a fetch error in the UI). No monitoring is in scope; treat breakage as a normal bug to fix when noticed.

## 5. Data model (SQLite)

Six tables. `db::init` runs `CREATE TABLE IF NOT EXISTS` for all of them on every launch, plus a small `migrate()` step that `ALTER TABLE ADD COLUMN`s anything introduced after a table's original shape (guarded by a `pragma_table_info` check) — needed because `IF NOT EXISTS` only shapes brand-new databases, not ones already on disk from an earlier version.

```sql
-- `source` + `source_story_id` (composite-unique) replace the old FFN-only
-- `ffn_story_id UNIQUE` so a second source can coexist without id collisions
-- (§8, locked 2026-07-19). `category`/`warnings` are AO3-only, nullable,
-- unused by FFN rows — option (a) from §8's data-model gap: bolt-on columns
-- over a separate per-source tag table, cheap and matches how
-- `ffn_updated_at` was added. Existing (all-FFN) rows migrate via a table
-- rebuild in `db::migrate` (`ALTER TABLE` can't change a UNIQUE constraint);
-- `id` is preserved so `chapters`/`read_state` foreign keys stay valid.
CREATE TABLE stories (
    id              INTEGER PRIMARY KEY,
    source          TEXT NOT NULL DEFAULT 'ffn' CHECK (source IN ('ffn', 'ao3')),
    source_story_id INTEGER NOT NULL,          -- FFN's/AO3's own story id, from the URL
    title           TEXT NOT NULL,
    author          TEXT NOT NULL,
    fandom          TEXT NOT NULL,
    rating          TEXT NOT NULL,
    genre           TEXT,
    language        TEXT,
    tags            TEXT,                      -- character/pairing text, verbatim from FFN
    category        TEXT,                      -- AO3 only: M/M, F/F, Gen, etc. — NULL for FFN
    warnings        TEXT,                      -- AO3 only: comma-joined warning tags, verbatim — NULL for FFN
    status          TEXT NOT NULL CHECK (status IN ('complete', 'in_progress')),
    word_count      INTEGER NOT NULL,
    favorite_count  INTEGER NOT NULL,          -- FFN Favs or AO3 Bookmarks — not a merged ranking (§8, locked)
    chapter_count   INTEGER NOT NULL,
    summary         TEXT,
    ffn_updated_at  TEXT,                      -- raw "Updated:" text from FFN's metadata line, verbatim (not reparsed)
    last_fetched_at TEXT NOT NULL,             -- ISO8601; drives "check for updates"
    UNIQUE (source, source_story_id)
);

CREATE TABLE chapters (
    id           INTEGER PRIMARY KEY,
    story_id     INTEGER NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    chapter_num  INTEGER NOT NULL,
    title        TEXT,
    content      TEXT NOT NULL,                -- '' until fetched (a stub row); cleaned HTML once cached
    fetched_at   TEXT NOT NULL,                -- '' until fetched
    UNIQUE (story_id, chapter_num)
);

CREATE TABLE read_state (
    story_id         INTEGER PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    is_read          INTEGER NOT NULL DEFAULT 0,   -- manual toggle
    marked_at        TEXT,
    last_chapter_num INTEGER,                      -- resume-reading position
    last_opened_at   TEXT,                          -- stamped on every chapter open/nav; feeds Continue Reading
    dismissed        INTEGER NOT NULL DEFAULT 0,    -- "not for me"
    dismissed_at     TEXT,
    bookmarked       INTEGER NOT NULL DEFAULT 0,    -- browsable in the Bookmarks screen
    bookmarked_at    TEXT
);

CREATE TABLE filter_presets (   -- unused (§1) — kept, no harm
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    filters_json TEXT NOT NULL
);

-- Tracks FFN discovery searches so "Load more" resumes from the next page
-- instead of re-fetching what's already imported. Sentinel non-null
-- defaults (0 / 'any') on the filter columns, not NULL — SQLite's UNIQUE
-- treats NULL as distinct from NULL, which would silently break upserts
-- for "no filter" searches.
CREATE TABLE browse_state (
    id                 INTEGER PRIMARY KEY,
    category           TEXT NOT NULL,             -- also the only place a fandom's FFN category is remembered
    fandom             TEXT NOT NULL,
    min_word_count     INTEGER NOT NULL DEFAULT 0,
    status             TEXT NOT NULL DEFAULT 'any',
    last_page_fetched  INTEGER NOT NULL DEFAULT 0,
    last_fetched_at    TEXT NOT NULL,
    UNIQUE (fandom, min_word_count, status)
);

-- Cached per-category fandom list (FFN's own category index page) for the
-- "Search FFN" picker. Only fandoms with >= 1,000 fics. Wholesale-replaced
-- per category on each (re)index, not merged — the index page is already
-- the complete current list in one fetch.
CREATE TABLE fandom_index (
    category    TEXT NOT NULL,
    fandom      TEXT NOT NULL,
    ffn_count   INTEGER NOT NULL,
    indexed_at  TEXT NOT NULL,
    PRIMARY KEY (category, fandom)
);
```

Notes:
- `(stories.source, stories.source_story_id)` is the natural key tying cached data back to its source; internal `id` is the FK target everywhere else. The Rust/frontend layer exposes this column as `source_story_id`/`sourceStoryId` (`queries.rs`'s `StoryRow`) — the FFN work id for `"ffn"` rows, the AO3 work id for `"ao3"` rows.
- `read_state` holds all per-story manual flags/timestamps, independent of `stories` (which only changes on refetch) — so re-importing a story never resets your read/dismiss/bookmark state.
- `chapters` rows are created as empty stubs (`content = ''`) for the *entire* TOC every time any page of a story is fetched (any story page lists every chapter), then filled in individually as each chapter is actually opened. `content <> ''` is the "is this chapter cached" check used throughout.

## 6. UI/UX

Three screens: **Search**, **Bookmarks**, **Reader** (Reader is not a persistent tab — it opens full-screen from a story click in either list or from Continue Reading, with Back returning to whichever tab you came from). A **theme toggle** (Light/System/Dark, `ThemeToggle.svelte`, persisted to `localStorage`, applied via a `data-theme` attribute on `<html>`) sits top-right on every screen including the Reader.

### Search

- **Fandom picker**: separate control from the text search — opens a popover with a type-to-filter list of fandoms *already imported locally*. Fandoms can be **pinned** (inline star toggle, floats to a "Pinned" section) — persisted to `localStorage` (`pinnedFandoms.ts`), same pattern as reader typography and theme prefs. A small **rename/merge** control (pencil icon, `FandomPicker.svelte`'s `onRename`) renames a fandom bucket across every story that has it, merging it into an existing one if the new name already exists — the cleanup path for two situations §8 hit in practice: an AO3 import that kept AO3's own canonical tag text instead of the name already used on the FFN side, and AO3 crossover works whose `fandom` is a comma-joined multi-fandom string. `queries::rename_fandom` (`UPDATE stories SET fandom = ... WHERE fandom = ...`), only wired into Search's picker (not Bookmarks').
- **Search FFN button**: opens the discovery modal (below) — the actual mechanism for finding *new* fandoms/stories, separate from the fandom picker above (which only filters what's already local).
- **Text search**: filters by title, author, or summary only — never fandom.
- **Filters**, laid out on a second filter-bar row (source/length/status/hide-checkboxes; fandom/search/sort/view-toggle stay on the first row): source (All/FFN/AO3), word-count minimum (Any / 40K+ / 100K+), status (Any / Done / Ongoing — "Complete"/"In progress" internally, relabeled shorter throughout the UI), sort (Favorites / Updated / Words), and three independent checkboxes — "Hide read", "Hide dismissed" (both default on), "Hide bookmarked" (default off) — these narrow what's already imported; they don't fetch anything. All three checkbox states and the list/card view mode (below) persist to `localStorage` (`filterPrefs.ts`/`viewMode.ts`) across restarts.
- **Search is accent-insensitive**: the fandom picker, the Search FFN modal's fandom picker, and the title/author/summary text search all fold case and strip common Latin diacritics before matching (`fold()` in `normalize.ts` on the frontend; a matching `fold()` in `queries.rs` for the backend text search, applied to the already-SQL-narrowed result set since SQLite's `LIKE` has no accent-folding) — so "poke" matches "Poké".
- **"Ongoing" status note**: list/card rows for an ongoing story show FFN's own "Updated:" info (from `stories.ffn_updated_at`) next to the status pill, when known, as a small `Calendar` icon + compact date. `ffn_updated_at` is stored verbatim (not reparsed at fetch time — FFN's exact format wasn't confirmed against enough live markup at first). Display-time parsing (`shortDate()` in `format.ts`) now handles the three shapes since confirmed against real data: a relative age shown as-is ("2h"), a bare "M/D" with no year (assumed to be this year, rolled back one year if that would land in the future), and a full "M/D/YYYY". Anything else returns `null` and the badge just doesn't render. `null`/absent for one-shots and stories never refetched since this field was added.
- **Refresh button** (top bar, left of the theme toggle, visible on both Search and Bookmarks): re-fetches the chapter-1 page of every **bookmarked, not-yet-complete** story — refreshes its metadata (favorites, word/chapter count, `ffn_updated_at`, and status if it's actually finished now) and picks up any new chapters, same mechanism as any other story-page fetch. Paced 1.5–3s apart like any other multi-fetch operation; a spinning icon and a "Refreshing N of M…" tooltip track progress; one story failing doesn't stop the rest.
- **List/card view toggle**: Lucide `List`/`LayoutGrid` icons (not text/glyphs), shared (one persisted preference, `viewMode.ts`) between Search and Bookmarks rather than tracked per-screen. *List* — title + author/status stacked left, single-line truncated description (with hover tooltip for the full text) and truncated tags line in the middle (row height fixed), word count + favorites right (each with a small icon — `WholeWord` / `Heart` — instead of a text suffix), then row actions. *Card* — fandom tag, title, full summary, tag chips, **then** rating/words/favorites tags (moved below the chips so every card's footer lines up regardless of how many chips it has), status pill, same row actions.
- **Load more**: shown at the bottom of results whenever a specific fandom is selected — fetches the next page for the current fandom+filter combo. FFN via `browse_state`; AO3 via `ao3_browse_state` (§8) — which source "Load more" continues is the **Source filter** if set to one source, else whichever source the last modal search used. Also surfaces as an **empty-state nudge** ("you've gone through everything imported for this search") when the current filters leave nothing visible.
- **Source filter** (All/FFN/AO3, §8): scopes Search's results to one source or both — locked design decision that sources aren't merged into one ranking (§8) means sort-by-favorites only makes sense within one source at a time. Each row also carries a small **source badge** (list and card view) so a mixed result set stays legible.

### Search modal (`SearchModal.svelte`, generalized 2026-07-19 from `SearchFfnModal.svelte`)

Discovery entry point for both sources, opened by the **Search** button. A source toggle (FanFiction.net / Archive of Our Own) at the top swaps only the fandom-input section underneath — everything else (word-count/status filters, submit flow) is shared.

- **FFN**: unchanged from before — category selector defaults to **"All categories"** (searches/browses everything indexed so far, sorted by popularity globally) or a specific category; fandom field is a type-to-search picker sourced from the cached `fandom_index`, not free text (a typo there is a silent 404). If a category hasn't been indexed yet, a button fetches it (`index_ffn_category`); "index all categories" bulk-fetches all nine sequentially with a delay between each.
- **AO3**: no cached tag index exists (`robots.txt` disallows AO3's `/autocomplete/` endpoint, so it can't be built the same way §8 built FFN's). Two free-text fields instead: a local **fandom name** and the **AO3 tag** itself (AO3's exact tag string, e.g. "Harry Potter - J.K. Rowling" for "Harry Potter" — doesn't slugify predictably from a fandom name). The tag is saved to a `localStorage` map (`ao3TagMap.ts`, same preference pattern as `pinnedFandoms.ts`) keyed by fandom name after a successful search, auto-filled next time — so it's typed once per fandom, not every search. A typo in the tag just returns AO3's own empty/wrong result, not a silent 404. **The fandom name isn't just a label** — it's force-applied to every imported row's `fandom` column (`search_ao3`/`try_persist_ao3_fetch`), overwriting AO3's own parsed fandom text, same as FFN's browse import already force-applies one URL-derived fandom to every row (§4). Found live (2026-07-19) why this matters: without it, a "Harry Potter" search imported real stories correctly but under AO3's canonical text ("Harry Potter - J. K. Rowling"), which doesn't exact-match the FFN-side fandom filter — two fandom buckets for one fandom. `rename_fandom` (above) is the cleanup for anything already imported before this was fixed.
- Word-count/status filters scope the actual query for whichever source is selected (§4 for FFN, §8 for AO3 — AO3 additionally offers an "Ongoing" status option, confirmed working server-side, unlike FFN's unconfirmed one). On submit, imports page 1 and selects the resulting fandom in Search.

### Bookmarks

Same list/card rendering as Search, but:
- Its own fandom picker, scoped to fandoms actually present among bookmarks (defaults to "All fandoms").
- Its own "Hide read" checkbox (default off), persisted separately from Search's.
- List rows show fandom (entries span fandoms here).
- **No dismiss action** — only mark-read and un-bookmark; dismissing something already bookmarked isn't meaningful.
- **Continue Reading** strip at the top: up to 5 in-progress, non-dismissed stories you've actually opened, most-recently-opened first (`read_state.last_opened_at`) — click to jump straight back into the Reader at your last chapter.
- **Download all chapters** (row action, `Download` icon, bookmarks-only): force-fetches every not-yet-cached chapter of that one story ahead of time, for fully offline reading — no new backend command, just `listChapters` + a loop over `ensureChapterCached` (`chapterCache.ts`, the same helper the Reader's on-demand fetch and prefetch use), paced 1.5–3s apart like Refresh/prefetch, best-effort per chapter. Runs as an ordinary async loop against page-level state, so it keeps going in the background across screen switches (reading a different story, etc.) — same as Refresh. Only one story downloads at a time (`downloadingStoryId` gates the row buttons). A top-of-screen banner (title + progress bar) shows live progress, not just a hover tooltip. `fetch_page` (`src-tauri/src/lib.rs`) now reuses one hidden `fetch-page` window across every fetch for the app session (`navigate()`) instead of destroying/rebuilding it per chapter — on macOS, *creating* a window activates the app regardless of `.focused(false)`/`.visible(false)` (those only govern focus once already frontmost), so rebuilding a window per chapter yanked OS focus back every time. `report_fetch_result` used to undo this by `close()`-ing the window on every successful fetch (leftover from before reuse existed), which forced the very next `fetch_page` back through `build()` anyway — removed, so the window now only builds once per app session and every fetch after that is a pure `navigate()` with no window-server calls at all.
- **"Offline" badge** (list meta-line / card fandom-row, bookmarks-only): a small moss-colored pill next to the source badge when every chapter of a story is cached (`StoryRow.cachedChapterCount === chapterCount`, a `(SELECT COUNT(*) FROM chapters WHERE content <> '')` subquery added to the shared `STORY_COLUMNS` in `queries.rs`, so it's available on every `StoryRow`-returning query, not just Download-all's own check) — makes it visible at a glance which bookmarked stories are already fully downloaded. The Download button itself hides once a story reaches this state (nothing left to fetch) and reappears on its own if a Refresh later adds new chapter stubs to an ongoing story.

### Reader

Immersive layout: progress bar (tracks actual scroll position within the chapter, not just chapter-count ratio), chapter prose in a centered serif column, no persistent chrome. Corner controls fade in on hover only:
- **Top-left**: Back, a Chapters button (slide-in drawer listing the full TOC, unfetched chapters marked), and an **Aa** button opening the typography panel.
- **Top-right**: Theme toggle only.
- **Bottom-right `.fab-cluster`**: three stacked circular buttons — Bookmark and Dismiss (smaller, secondary, outlined) above the primary Mark-as-read (larger, dark fill). All fade to a faint `0.3` opacity at rest, full opacity on individual hover — same shape/fade language across all three, not just the primary one.
- **Left/right edges**: click zones for previous/next chapter; **←/→ arrow keys** do the same (ignored while a modifier key is held).
- A **hover-reveal chapter carousel** on the left edge: dots sized to fit the window's height (`NAV_RADIUS` derived from `innerHeight`, minimum 2 either side of the current chapter — taller windows show more), current chapter centered. Each dot fades by distance from the current chapter (`1 − distance × 0.16`, floored at `0.25`) via a `--dist-opacity` CSS custom property; hovering a specific dot restores it to full opacity regardless of distance. Labels read `"{chapterNum}. {title}"` and reveal on hover — layered above the edge-click zones (paints later in the DOM, same stacking context) so it doesn't get swallowed by them.
- Each chapter renders as one continuous scroll — no artificial in-chapter pagination. Opens at the resume-reading position (`read_state.last_chapter_num`) when returning to an in-progress story. Uncached chapters are fetched on demand (§3) when navigated to. **Per-chapter scroll position is remembered for the session** (an in-memory `Map<chapterNum, scrollTop>`, not persisted) — paging back to a chapter you were partway through resumes there instead of jumping to the top; restored via `requestAnimationFrame` after the chapter's DOM settles, since `tick()` alone doesn't guarantee the browser has finished laying out the newly-inserted HTML. The scroll container is also given keyboard focus (`tabindex="-1"` + `.focus({ preventScroll: true })`) on every chapter load, so Space/arrow-key scrolling works immediately without an extra click first.
- **Chapter prefetch**: after a chapter loads, the two chapters before and two after it are quietly fetched in the background if not already cached — same fetch mechanism and Cloudflare-safe pacing as any bulk fetch (§3), skipped/abandoned the moment you navigate again (a generation counter invalidates a stale prefetch chain rather than letting it race a real navigation).

### Reader typography panel

Persisted to `localStorage` (`readerPrefs.ts`), applied live via inline styles on the prose element: font family (Serif/Sans toggle, or a **custom font name** typed in — e.g. "Bookerly" — used if installed locally, falling back to the Serif/Sans choice otherwise; nothing is downloaded/bundled), size (14–28px), weight and bold weight (two independent 100–900 steppers, not a fixed Light/Regular/Bold toggle — the bold-weight stepper is wired to a `--bold-weight` CSS variable applied to `<b>`/`<strong>` inside the prose, since those otherwise ignore the base weight and render at the browser's fixed default bold), line height (1.2–2.4), letter-spacing/kerning (−0.05 to 0.2em), page width (40–90ch), and a **smart quotes** toggle (default on) that converts straight quotes/apostrophes to curly ones at render time (`typography.ts`'s `smartenQuotesHtml`, operates on parsed text nodes so it can't corrupt chapter HTML markup). Laid out as a 2-column grid (label | control) with controls stretched to fill the column, not left-aligned with trailing dead space.

## 7. Decision trail

Full detail, research sources, and the review rounds behind each locked decision live in the closed tickets under `wayfinder/tickets/` (map: `wayfinder/map.md`). That trail covers the map's original scope; everything past §6's Search FFN modal onward (theme toggle, reader typography, discovery/fandom-index) was designed and built directly in conversation after the map closed, without its own tickets.

## 8. Multi-source planning: Archive of Our Own (in progress)

Everything in §1–§7 was originally FFN-specific by construction — the original schema (`ffn_story_id`, `fandom` as a single URL-derived string), the fixed 9-category browse taxonomy, and the whole Cloudflare/webview fetch strategy (§3) all assumed one source. Adding AO3 (archiveofourown.org) as a second source was a real architectural change, not a second scraper bolted on. Schema, parser, upsert path, fetch path, and UI wiring (Search modal, source filter/badge, Reader) are now all built and live-tested — see "Progress & next steps" at the end of this section for what's actually left.

**Fetch strategy is genuinely different, confirmed by live test (2026-07-18):** a bare HTTP request (`urllib`, default headers, no cookies, no JS) against AO3's homepage, a tag-scoped work listing, and a full Mature-rated work page all returned real HTML with no Cloudflare challenge, no captcha, no block page. This is the opposite of FFN's situation (§3) — AO3 doesn't appear to run the same bot-fingerprinting defense. Practically: AO3 support could use the `reqwest` dependency that's been sitting unused since v1 (§2), skipping the webview/window-management machinery entirely for this source.

**Correction, live-tested 2026-07-19 while building the fetch path**: the above holds only with a realistic User-Agent header. Re-tested from a different network than 2026-07-18's: both `reqwest`'s and `urllib`'s *default* UA strings got a genuine Cloudflare "managed challenge" (403, "Shields are up!") on both a work page and a tag listing — not the origin flakiness documented below, an actual UA-based block. Setting a real desktop-browser UA string fixed both on the first try, confirmed with `urllib` and then with `reqwest` directly. So AO3's Cloudflare does do *some* filtering — just UA-based, not full TLS/JS fingerprinting the way FFN's does — and a static header clears it with no webview/interactive-challenge handling needed. `fetch_ao3_html` (`lib.rs`) sends a hardcoded browser UA for this reason.

**Explicit-rated works confirmed no gate (live test, 2026-07-18)**: fetched `/works/85954646` ("Perks of Being Kidnapped", rating tag confirmed `Explicit`, found via tag-scoped browse of `/tags/Naruto.../works`) with the same bare `urllib`, zero cookies, zero auth. Full chapter text returned directly in `div.userstuff` — no login/age-confirmation interstitial. The sidebar login form present on every AO3 page is just the site nav widget, not a content gate. Full-catalog parity (Mature + Explicit) confirmed reachable without an authenticated session.

**AO3 origin reliability is worse than FFN's, independent of bot-blocking (live test, 2026-07-18)**: across a handful of bare requests, hit two plain `TimeoutError`s and one `HTTP 525` (Cloudflare origin SSL error) before a retry succeeded — no challenge page or captcha involved, just origin flakiness. Any AO3 fetch path needs retry-with-backoff on timeout/5xx; FFN's webview strategy never needed this since it only needs inter-request pacing (§3), not retries.

**robots.txt scope** (fetched live): disallows the *global* `/works?` search, `/autocomplete/`, `/downloads/`, `/external_works/`, and any `*/search?` endpoint for all crawlers, and blocks AI-training bots (GPTBot/CCBot/ChatGPT-User) wholesale. It does **not** disallow tag-scoped browsing (`/tags/{fandom}/works?...`) or individual work pages (`/works/{id}`).

**Revised, 2026-07-19**: turned out the boundary above doesn't actually hold word-count/status filtering. Confirmed live that `/tags/{fandom}/works` silently ignores every `work_search[...]` param — AO3's own filter sidebar (`#work-filters`) posts to `action="/works"` with the tag as a `tag_id` hidden field, i.e. the disallowed global path, scoped only by that query param. There's no filtered shape that stays inside `/tags/{fandom}/works`. User-confirmed decision: use `/works?tag_id=...` anyway, same personal-use-scale posture already applied to FFN's Cloudflare/ToS situation (§1) — it's exactly what a human's browser sends when clicking "Sort and Filter" on a tag page, just via `reqwest`. See `build_tag_browse_url` (`src-tauri/src/ao3.rs`).

**Data model gap — locked (a), implemented 2026-07-19**: AO3's tagging is richer than what the schema held. Confirmed live markup shows, per work: rating, one or more **warnings** (multi-select, e.g. "Major Character Death, Graphic Depictions Of Violence" — FFN has none of this), a **category** (M/M, F/F, Gen, etc. — FFN has none of this), separate **relationship** and **character** tag groups (FFN's `tags` column mushes character/pairing into one free-text string), and arbitrary **freeform** tags on top of all that. Weighed (a) a `source` column plus AO3-specific nullable columns bolted onto `stories` against (b) a proper per-source tag table; went with (a) — cheap, keeps one table, mirrors how `ffn_updated_at` was added, and (b)'s payoff is speculative until a third source shows up. Schema now has `source`/`category`/`warnings` (§5); `relationship`/`character`/`freeform` tag splitting is deferred to whenever the AO3 parser is actually written — `tags` (mushed, FFN-style) is good enough until then. `ffn_story_id UNIQUE` became `(source, source_story_id) UNIQUE`, migrated via a table rebuild in `db::migrate` (verified against a copy of the real 266-row local DB: all ids/rows/FKs preserved, `PRAGMA integrity_check` clean).

**Markup skeleton, confirmed — parser written 2026-07-19** (`src-tauri/src/ao3.rs`): listing rows are `<li class="work ...">` — title+id `h4.heading a[href^="/works/"]`, author(s) `h4.heading a[rel="author"]`, fandom(s) `h5.fandoms.heading a.tag`. Rating/category/complete-status read off the `title` attribute of `ul.required-tags li span.{rating,category,iswip}` (the human-readable string, e.g. `title="Teen And Up Audiences"`) rather than reverse-mapped from the `rating-*`/`category-*` class slug. Full-detail warnings/relationships/characters/freeforms come from `ul.tags.commas li.{warnings,relationships,characters,freeforms} a.tag`. Stats: `dl.stats dd.{words,chapters,kudos,comments,bookmarks,hits}`, `dd.chapters` holds `"N/M"` (posted/planned, `M` = `"?"` if undetermined). Work pages: `h2.title.heading`, `h3.byline.heading a[rel="author"]`, full tag detail in `dl.work.meta.group dd.{rating,warning,category,fandom,relationship,character,freeform}.tags a.tag`, chapter TOC in `select#selected_id option` (`value` = AO3's opaque per-chapter id, used to build `/works/{id}/chapters/{chapter_id}` fetch URLs — chapters aren't sequentially numbered in the URL like FFN's `/s/{id}/{n}/`), chapter body in `div.userstuff.module[role="article"]` (distinguishes it from the *other* `.userstuff` divs on the same page — author's notes before/after the chapter). **Gotcha found while writing the parser**: `dl.work.meta.group` is not actually nested inside `#workskin` in html5ever's strict parse tree (some unbalanced tag upstream closes `#workskin` earlier than a browser's lenient rendering would suggest) — those selectors query the whole document instead of scoping under `#workskin`, safe since a work page only ever has one work.

**Locked**: sources are *not* merged into one popularity ranking. FFN's "Favs" and AO3's engagement metrics aren't the same signal at all — AO3's kudos is a one-click, near-frictionless "like" (315,868 kudos vs. only 56,015 bookmarks on the same test work), nothing like FFN's deliberate Favorite action. AO3's **bookmarks** is the closer analog to FFN's Favorites (both a deliberate "save/track this" action), not kudos — but even that likely isn't a fair apples-to-apples scale given the platforms' different audience sizes. Each source gets its own filter/badge and sorts within itself; "sort by favorites" across a mixed list isn't a feature that should exist.

### Progress & next steps (as of 2026-07-19)

**Done:**
- Fetch strategy, Explicit-content gate, origin reliability, robots.txt scope — all confirmed live (above).
- Schema: `stories.source`/`source_story_id`/`category`/`warnings` implemented and migrated (§5), verified against a copy of the real local DB.
- Parser written and verified against live-captured markup: `src-tauri/src/ao3.rs` — `parse_work_page`, `parse_listing_page`, `parse_chapter_content`, `work_id_from_url`, `chapter_id_from_url`, `build_tag_browse_url`.

**Done (2026-07-19, cont.):**
- `chapters.source_chapter_id` (nullable `INTEGER`) added — both to `SCHEMA` (new DBs) and via `add_column_if_missing` in `migrate()` (existing DBs) — so `Ao3ChapterRef.chapter_id` has somewhere to persist.
- **Found and fixed a live data-integrity bug while verifying the above against a copy of the real DB**: the *previous day's* `migrate_stories_source_column` (the `stories` rebuild, §5) used a plain `RENAME TO stories_old`, which triggers SQLite's default auto-rewrite of *other* tables' FK clauses that pointed at `stories` — so `chapters`/`read_state` silently ended up with `REFERENCES "stories_old"(id)`, a table that gets dropped two statements later. Confirmed against the real DB: 1480 `pragma_foreign_key_check` violations (all 1214 `chapters` + all 266 `read_state` rows) — `ON DELETE CASCADE` had been silently broken since that migration ran. Fixed two ways: `migrate_stories_source_column` now wraps its rename in `PRAGMA legacy_alter_table = ON` so it can't recur, and a new one-time `fix_dangling_stories_fk` (detects the dangling reference via `pragma_foreign_key_list`, not a version flag, so it's a no-op once repaired) rebuilds `chapters`/`read_state` for DBs that already have the bug baked in. Re-verified against a fresh copy of the real 266-story/1214-chapter DB: `integrity_check` clean, `foreign_key_check` clean (0 violations), all rows preserved.

**Done (2026-07-19, cont.):**
- AO3 upsert path: `db::upsert_ao3_story` (`src-tauri/src/db.rs`) — separate function from `upsert_story` rather than a shared one taking `source` explicitly, since the two metadata shapes barely overlap (AO3 has `category`/`warnings`, no `genre`/`ffn_updated_at`; FFN the reverse). Same re-fetch contract: `ON CONFLICT(source, source_story_id)` refreshes cached metadata, never touches `read_state`. `upsert_chapter_stub`/`upsert_chapter_content` gained a `source_chapter_id: Option<i64>` param (FFN call sites in `lib.rs` pass `None`; AO3 will pass `Ao3ChapterRef.chapter_id`) so AO3 chapters can persist the id needed to build `/works/{id}/chapters/{chapter_id}` fetch URLs. 6 new unit tests (AO3 row shape, refetch-preserves-read_state, FFN/AO3 coexistence under the same `source_story_id`, chapter `source_chapter_id` round-trip for both sources) — all pass; `cargo check` clean.

**Done (2026-07-19, cont.):**
- AO3 fetch path: `fetch_ao3_html` + `try_persist_ao3_fetch` + `fetch_ao3_page` Tauri command (`src-tauri/src/lib.rs`). Direct `reqwest` GET with retry-with-backoff (4 attempts, 500ms/1s/2s) on connect/timeout errors and 5xx — no webview/window machinery, unlike FFN (§3), since a single async call can return its result directly instead of needing `report_fetch_result`'s event round-trip. **Found live while building this**: AO3's Cloudflare *does* filter on User-Agent — both `reqwest`'s and `urllib`'s default UA strings got a "managed challenge" 403 on both a work page and a tag listing (see the correction note above), fixed by sending a real desktop-browser UA. Dispatches on the post-redirect URL (`ao3::work_id_from_url` / new `ao3::tag_from_listing_url`) to either a work/chapter branch (`parse_work_page` + `parse_chapter_content`, chapter number resolved from the redirected chapter id against the work's own TOC) or a tag-listing branch (`parse_listing_page`), reusing `upsert_ao3_story`/`upsert_chapter_stub`/`upsert_chapter_content`/`ensure_read_state` from the upsert path above. Emits the same `chapter-fetched`/`browse-fetched`/`*-error` events `report_fetch_result` does, so future UI wiring doesn't need source-specific event handling. Verified live end-to-end (real network, this session): fetched the same 80-chapter test work (`/works/85954646` → redirects to `/works/85954646/chapters/227238471`, parsed correctly) and a live `Naruto` tag listing (20 works parsed); the retry logic fired for real on a transient connection error during that same run and recovered on the next attempt. AO3 pagination-resume (FFN's `browse_state`, §5) is *not* reused — its shape is FFN-specific and AO3's filter param names aren't confirmed yet (next item), so a listing fetch just imports whatever page URL it's given with no "load more" state tracked yet.

**Done (2026-07-19, cont.):**
- `build_tag_browse_url`'s word-count/status filter params, confirmed live: switched to `/works?tag_id={tag}&work_search[sort_column]=...&page=...` (see the robots.txt revision above), added `work_search[words_from]` (minimum word count — verified: `100000` returned only 6-digit+ word counts, min observed 105,880) and `work_search[complete]` (`"T"`/`"F"`/omitted for complete/in-progress/any — verified both `T` and `F` against each result's own `span.complete-yes`/`complete-no.iswip` badge, not just absence of the other value's badge). `tag_from_listing_url` (new, `ao3.rs`) recovers the tag from a `/works?tag_id=...` URL for `try_persist_ao3_fetch`'s dispatch, replacing the old `/tags/{tag}/works` path match. 4 new unit tests in `ao3.rs` (offline, string-shape only) plus the exact Rust-generated URL was hit live and round-tripped correctly. `cargo check`/`cargo test --lib` clean (10/10 pass).

**Done (2026-07-19, cont.) — UI wiring:**
- Decided (user-confirmed): one generalized `SearchModal.svelte` (renamed from `SearchFfnModal.svelte`) with a source toggle, not two separate modals — the two sources only really differ in how fandom/tag input works.
- Backend gaps closed first: `StoryRow`/`queries::StoryRow` gained `source` (was entirely absent — Reader had no way to know a story's source at all); `ChapterListItem` gained `source_chapter_id` (needed to build an AO3 chapter's `/works/{id}/chapters/{id}` URL); `StoryFilters` gained `source` for the new All/FFN/AO3 filter. All three verified with new `queries.rs` unit tests (column-index bugs here are exactly the class of silent-wrong-data risk this session already hit twice).
- AO3 pagination: new `ao3_browse_state` table + `next_ao3_browse_page`/`record_ao3_browse_page` (`db.rs`) — simpler than FFN's `browse_state` since AO3 has no category to resolve. `try_persist_ao3_fetch`'s listing branch now re-derives `(min_word_count, status, page)` from the fetched URL's query string (same pattern FFN's own listing branch already used) and records it. New `search_ao3` Tauri command mirrors `search_ffn`; sort is hardcoded to `bookmarks_count` (not `kudos_count` — SPEC's own locked note above says bookmarks, not kudos, is the analog to FFN's Favorites). Verified against a fresh copy of the real DB: schema migrates cleanly, `integrity_check`/`foreign_key_check` clean, row counts preserved.
- AO3 tag mapping: since there's no cached tag index (`robots.txt` blocks `/autocomplete/`), added `ao3TagMap.ts` — a `localStorage` map from fandom name → AO3 tag string (same pattern as `pinnedFandoms.ts`), typed once in the modal, reused automatically after. "Load more" looks up this map when continuing an AO3 search.
- Reader (`Reader.svelte`) no longer hardcodes an FFN URL: new `api.chapterFetchUrl(story, chapterNum, sourceChapterId)` branches on `story.source`, and `api.fetchChapterAndWait` takes an explicit source to pick `fetch_page` vs `fetch_ao3_page` (both emit the same events, so the awaiting logic didn't need to change). Falls back to AO3's bare `/works/{id}` URL (which redirects to chapter 1) when a chapter's `source_chapter_id` isn't known yet — covers a story only discovered via a tag listing (no per-chapter TOC yet) exactly the way FFN's own "TOC populates lazily on first open" already worked (§5).
- `StoryList.svelte` rows (list and card) show a small source badge.
- Verified: `cargo check`/`cargo test --lib` clean, `bun run check` clean, `bun run tauri dev` builds and launches with no runtime errors, real DB opens/migrates cleanly.

**Found and fixed (architecture review, 2026-07-19)**: `ao3::parse_chapter_content`'s `div.userstuff.module[role="article"]` selector included AO3's own fixed accessibility landmark heading (`<h3 class="landmark heading" id="work">Chapter Text</h3>`) as the first child — not per-chapter content, AO3's own screen-reader label for the region — showing up as a redundant heading above the real prose on every AO3 chapter. Fixed by detaching any `h3.landmark.heading` from the parsed tree before serializing; a one-time `db::fix_ao3_landmark_heading_in_chapters` migration backfilled the 9 already-cached chapters that had it baked in.

**Found and fixed (2026-07-19)**: two AO3 stories ("All the Young Dudes", "Draco Malfoy and the Mortifying Ordeal of Being in Love") had `summary = NULL` despite every other field (word/chapter count, tags, etc.) matching live AO3 data exactly — re-running the current parser against fresh live markup and the test fixtures extracted the summary correctly both times, so this wasn't a live parsing bug, most likely a transient truncated read at the original fetch time (`fetch_ao3_html` only retries on timeout/connect/5xx, not a successful-but-incomplete body). Backfilled both rows directly using summaries extracted by the real parser against freshly fetched live HTML. Separately noted: once a story's chapters are all cached, nothing re-fetches its metadata — the Reader only fetches chapters that aren't cached yet — so a gap like this can't self-heal by just reading the story again; no "refresh metadata for an already-fully-cached story" mechanism exists yet.

**Click-tested live by the user (2026-07-19) — found and fixed two real bugs:**
- **Tauri v2 permission gap**: `search_ao3`/`fetch_ao3_page` were registered in `invoke_handler!` but not in `permissions/main-commands.toml`'s `commands.allow` — rejected at `invoke()` time with no compile-time warning (see the new Architecture maintenance note, §2). Also hit, while fixing it: editing the `.toml` didn't trigger a rebuild (Cargo's build-script fingerprinting is content-based, not mtime-based) — same note.
- **Fandom bucket mismatch**: a live "Harry Potter" AO3 search imported real stories (confirmed: 10+ rows, correct titles/ids) but Search showed "you've gone through everything imported" — zero results. Root cause: `stories.fandom` was AO3's own canonical text ("Harry Potter - J. K. Rowling"), not the "Harry Potter" label typed into the modal, so Search's exact-match fandom filter never matched. Fixed at the root (not just this one case): `search_ao3`/`try_persist_ao3_fetch` now force-apply the typed fandom label to every imported row (`fandom_override` param), same as FFN's browse import already force-applies one URL-derived fandom to every row (§4) — see the Search modal AO3 bullet above.
- Follow-up (user-requested): a **rename/merge fandom** control (`FandomPicker.svelte`'s `onRename`, `queries::rename_fandom`) to clean up rows already imported before the fix above, and AO3 crossover works' comma-joined `fandom` buckets — see the Search fandom-picker bullet above. 1 new backend test (`rename_fandom_updates_matching_rows_and_merges_into_an_existing_one`); `cargo test --lib` 15/15 pass; `bun run check` 0 errors.

**Not done — pick up here:**
1. **Still-open design questions** (§8, unresolved): whether `fandom` becomes a proper set instead of a comma-joined string (AO3 crossovers are natively multi-fandom — the parser currently just joins, e.g. `"Original Work, Naruto (Anime & Manga)"`); whether FFN's fixed-category browse UX even makes sense for AO3's freeform tag space; AO3 has no generic "Updated:" tracking column yet (`ffn_updated_at` is FFN-only), so an AO3 story's "check for updates" story is undefined.
2. **`lastSearchSource`/tag-map session-scoping is a known v1 limitation** (`+page.svelte`): "Load more" only knows which source to continue via the Source filter or whichever source the modal last searched — a fandom picked straight from the fandom picker (not through the modal) in a session that's never touched AO3 for it defaults to assuming FFN. Same class of limitation FFN's own `next_browse_page` already had (falls back to "any prior search for the fandom"), not new, but not fully solved either.

**Open questions before any of this gets built**: whether "fandom" stays a single string or needs to become a proper set (AO3 crossovers list multiple fandoms natively, unlike FFN's one-fandom-per-story model); and whether the existing fandom-picker/category-browse UX (built entirely around FFN's fixed category tree) even makes sense for AO3's freeform tag space.

**Found and fixed (architecture review, 2026-07-29)**: `ffn::build_browse_url` can't encode `status == "in_progress"` distinctly from `"any"` (deliberate — no confirmed FFN query param for it, §4). `try_persist_fetch`'s listing branch used to re-derive the locator it recorded to `browse_state` by decoding the *fetched* page's own URL, which inherits that same ambiguity — so an in-progress-scoped FFN search always recorded under `status = "any"`, permanently desynced from `next_browse_page`'s lookup under the real `"in_progress"` filter, silently resetting "Load more" to page 1 forever for any such search. Fixed by threading the originally-requested `FfnListingLocator` through the fetch instead of reconstructing it after the fact: `search_ffn` builds it once and stashes it on `FetchLock` (renamed `PendingFetch`, now `{ url, listing_locator }`); `report_fetch_result` reads it back out (guarded against a superseding fetch having already overwritten it) and passes it to `try_persist_fetch`, which now records under it via a new `FicSource::resolve_listing_record_locator(decoded, external)` hook — `Ffn` prefers `external` (and skips recording entirely when `None`, e.g. the debug harness's raw-URL fetch, which has no real search session to bookkeep); `Ao3` keeps its existing decode-and-record behavior unchanged (its own URL encoding round-trips `status` losslessly, and it never stashes anything — no bug there). 4 new tests (2 `db.rs`: FFN pagination resume/no-regression, mirroring the existing AO3 coverage that exposed this gap; 2 `lib.rs`: the in-progress round-trip fix itself, and the no-stash-skips-recording case). `cargo test`/`bun run check`/`bun run test` all clean.

**Deepened (architecture review, 2026-07-29, no behavior change except the bullet below)**: `classify_fetch_target` (`lib.rs`) — previously hand-matched a runtime `FetchSource` enum to pick between `source::Ffn`/`source::Ao3` calls, duplicating (in a different order) the chapter-then-listing dispatch `try_persist_fetch<S: FicSource>` already does generically; the two could drift if a shape's check order changed in one but not the other. Now generic over `S: FicSource` itself, sharing one literal dispatch order via three new small `FicSource` methods (`SOURCE_TAG`, `chapter_locator_ids`, `resolve_target_fandom` — same "decoded locator + optional external hint" shape as `resolve_listing_record_locator` above). FFN's category-index shape (no AO3 equivalent) moved into a new `classify_ffn_fetch_target` wrapper, checked *first* to mirror `try_persist_ffn_fetch`'s own order (previously checked last) — true order parity between the two paths, not just shared code. **Behavior change**: `fetch_page`'s supersede/interrupt check, now `resolve_interrupted_target` (pulled free of `AppHandle`, same move as `looks_blocked`, so it's independently unit-tested), prefers an interrupted browse fetch's own stashed `PendingFetch.listing_locator` over decoding its URL — an interrupted in-progress-scoped FFN search's error event now reports the right fandom without depending on URL decode at all. 2 new tests for `resolve_interrupted_target`; existing `classify_*` tests updated to the new generic/wrapper call shape. `cargo test` clean (58/58).
