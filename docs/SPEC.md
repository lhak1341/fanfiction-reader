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
  - `reqwest` — added as a dependency but never actually used as a fetch path; the webview strategy (§3) turned out sufficient for everything, including discovery pages
- **lucide-svelte** — icon set used throughout instead of unicode glyphs.

## 3. Fetching & Cloudflare strategy

fanfiction.net sits behind Cloudflare bot protection.

**Decision**: fetch through **Tauri's own WebView** (WKWebView on macOS), not a bare HTTP client — confirmed necessary: bare `reqwest`/`curl` are blocked by TLS/connection fingerprinting even with a copied `cf_clearance` cookie, and the mobile subdomain (`m.fanfiction.net`) gets no lighter protection than `www.` (FanFicFare's own adapter rewrites `m.` URLs to `www.` before parsing, confirming this). All scraping targets `www.fanfiction.net` markup.

**How it actually works** (`fetch_page` command in `lib.rs`): opens a Tauri webview window (label `fetch-page`) pointed at the target URL. **Hidden by default** (`.visible(false)`) — a real window flashing on screen for every chapter fetch/prefetch was disruptive; the tradeoff is that if fanfiction.net ever serves an interactive Cloudflare challenge instead of passing straight through (rare once `cf_clearance` is established), there's no visible window to solve it in and the fetch just times out — flip `.visible(false)` back to `true` temporarily to debug that case. An **initialization script** (runs before the page's own JS, on every navigation) listens for the page's `DOMContentLoaded` event — not `load`, and not Tauri's own `on_page_load(Finished)` delegate event, both of which proved unreliable in testing: pages with slow/hanging subresources (ad/tracking iframes, long-polling) never fire `load`, and Tauri's navigation-delegate `Finished` event didn't fire at all on some pages (browse/category listings in particular) despite the content rendering correctly on screen. `DOMContentLoaded` plus a 4-second fallback timeout proved reliable. On fire, the script invokes `report_fetch_result` with the page's HTML, URL, and title; that command dispatches on **URL shape** to one of three parsers (§4) and persists the result, then emits a success/error event and closes the window. A single `fetch-page` window is reused per fetch, so only one fetch is ever in flight — this also means rapid repeated fetches (e.g. bulk chapter prefetching, or the Reader's own chapter prefetch below) can look bot-like to Cloudflare; the app spaces out any multi-fetch operation by 1.5–3s between requests. Reuse is guarded by a `FetchLock` (tokio `Mutex`) around the destroy-old/build-new sequence, plus a short poll confirming the old window is actually gone before reusing the label — without it, two `fetch_page` calls landing close together (a background prefetch overlapping a foreground navigation) could race and crash with "a webview with label `fetch-page` already exists"; `destroy()` is used instead of `close()` for the same reason (`close()` goes through an interceptable, not-necessarily-immediate event pipeline).

**Blocked-page detection** is scoped to the `<title>` only (`"just a moment"`, `"attention required"`, `"checking your browser"`), plus a full-page check for the `cf-browser-verification` CSS class. Checking the *whole page text* for these phrases was tried first and produced false positives — ordinary story prose can innocently contain the phrase "just a moment."

## 4. Scraping

Three page types are parsed, all in `src-tauri/src/ffn.rs`, dispatched on URL shape in `try_persist_fetch` (`lib.rs`):

| URL shape | Example | Parser |
|---|---|---|
| `/s/{id}/{chapter}/...` | `/s/8067258/1/Worth-Dying-For` | `parse_story_page` + `parse_chapter_content` |
| `/{category}/{fandom}/...` | `/anime/Naruto/?srt=4&len=100&p=1` | `parse_browse_page` |
| `/{category}/` | `/anime/` | `parse_fandom_index` |

Selectors below are real, verified against live-fetched markup (not guessed from FanFicFare's Python source, though the field mapping was ported from its `adapter_fanfictionnet.py`).

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
CREATE TABLE stories (
    id              INTEGER PRIMARY KEY,
    ffn_story_id    INTEGER NOT NULL UNIQUE,   -- FFN's own story id, from the URL
    title           TEXT NOT NULL,
    author          TEXT NOT NULL,
    fandom          TEXT NOT NULL,
    rating          TEXT NOT NULL,
    genre           TEXT,
    language        TEXT,
    tags            TEXT,                      -- character/pairing text, verbatim from FFN
    status          TEXT NOT NULL CHECK (status IN ('complete', 'in_progress')),
    word_count      INTEGER NOT NULL,
    favorite_count  INTEGER NOT NULL,
    chapter_count   INTEGER NOT NULL,
    summary         TEXT,
    ffn_updated_at  TEXT,                      -- raw "Updated:" text from FFN's metadata line, verbatim (not reparsed)
    last_fetched_at TEXT NOT NULL              -- ISO8601; drives "check for updates"
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
- `stories.ffn_story_id` is the natural key tying cached data back to the source; internal `id` is the FK target everywhere else.
- `read_state` holds all per-story manual flags/timestamps, independent of `stories` (which only changes on refetch) — so re-importing a story never resets your read/dismiss/bookmark state.
- `chapters` rows are created as empty stubs (`content = ''`) for the *entire* TOC every time any page of a story is fetched (any story page lists every chapter), then filled in individually as each chapter is actually opened. `content <> ''` is the "is this chapter cached" check used throughout.

## 6. UI/UX

Three screens: **Search**, **Bookmarks**, **Reader** (Reader is not a persistent tab — it opens full-screen from a story click in either list or from Continue Reading, with Back returning to whichever tab you came from). A **theme toggle** (Light/System/Dark, `ThemeToggle.svelte`, persisted to `localStorage`, applied via a `data-theme` attribute on `<html>`) sits top-right on every screen including the Reader.

### Search

- **Fandom picker**: separate control from the text search — opens a popover with a type-to-filter list of fandoms *already imported locally*. Fandoms can be **pinned** (inline star toggle, floats to a "Pinned" section) — persisted to `localStorage` (`pinnedFandoms.ts`), same pattern as reader typography and theme prefs.
- **Search FFN button**: opens the discovery modal (below) — the actual mechanism for finding *new* fandoms/stories, separate from the fandom picker above (which only filters what's already local).
- **Text search**: filters by title, author, or summary only — never fandom.
- **Filters**: word-count minimum (Any / 40K+ / 100K+), status (Any / Done / Ongoing — "Complete"/"In progress" internally, relabeled shorter throughout the UI), sort (Favorites / Updated / Words), "hide read & dismissed" checkbox — these narrow what's already imported; they don't fetch anything.
- **Search is accent-insensitive**: the fandom picker, the Search FFN modal's fandom picker, and the title/author/summary text search all fold case and strip common Latin diacritics before matching (`fold()` in `normalize.ts` on the frontend; a matching `fold()` in `queries.rs` for the backend text search, applied to the already-SQL-narrowed result set since SQLite's `LIKE` has no accent-folding) — so "poke" matches "Poké".
- **"Ongoing" status note**: list/card rows for an ongoing story show FFN's own "Updated:" info (from `stories.ffn_updated_at`) next to the status pill, when known, as a small `Calendar` icon + compact date. `ffn_updated_at` is stored verbatim (not reparsed at fetch time — FFN's exact format wasn't confirmed against enough live markup at first). Display-time parsing (`shortDate()` in `format.ts`) now handles the three shapes since confirmed against real data: a relative age shown as-is ("2h"), a bare "M/D" with no year (assumed to be this year, rolled back one year if that would land in the future), and a full "M/D/YYYY". Anything else returns `null` and the badge just doesn't render. `null`/absent for one-shots and stories never refetched since this field was added.
- **Refresh button** (top bar, left of the theme toggle, visible on both Search and Bookmarks): re-fetches the chapter-1 page of every **bookmarked, not-yet-complete** story — refreshes its metadata (favorites, word/chapter count, `ffn_updated_at`, and status if it's actually finished now) and picks up any new chapters, same mechanism as any other story-page fetch. Paced 1.5–3s apart like any other multi-fetch operation; a spinning icon and a "Refreshing N of M…" tooltip track progress; one story failing doesn't stop the rest.
- **List/card view toggle**: Lucide `List`/`LayoutGrid` icons (not text/glyphs). *List* — title + author/status stacked left, single-line truncated description (with hover tooltip for the full text) and truncated tags line in the middle (row height fixed), word count + favorites right (each with a small icon — `WholeWord` / `Heart` — instead of a text suffix), then row actions. *Card* — fandom tag, title, full summary, tag chips, **then** rating/words/favorites tags (moved below the chips so every card's footer lines up regardless of how many chips it has), status pill, same row actions.
- **Load more from FFN**: shown at the bottom of results whenever a specific fandom is selected — fetches the next page for the current fandom+filter combo (via `browse_state`). Also surfaces as an **empty-state nudge** ("you've gone through everything imported for this search") when the current filters leave nothing visible.

### Search FFN modal (`SearchFfnModal.svelte`)

Discovery entry point. Category selector defaults to **"All categories"** (searches/browses everything indexed so far, sorted by popularity globally — lets you browse when you don't know exactly what to search for) or a specific FFN category. Fandom field is a type-to-search picker sourced from the cached `fandom_index`, not free text (a typo there is a silent 404). If a category hasn't been indexed yet, a button fetches it (`index_ffn_category`); "index all categories" bulk-fetches all nine sequentially with a delay between each. Word-count/status filters here scope the actual FFN query (§4); on submit, imports page 1 and selects the resulting fandom in Search.

### Bookmarks

Same list/card rendering as Search, but:
- Its own fandom picker, scoped to fandoms actually present among bookmarks (defaults to "All fandoms").
- List rows show fandom (entries span fandoms here).
- **No dismiss action** — only mark-read and un-bookmark; dismissing something already bookmarked isn't meaningful.
- **Continue Reading** strip at the top: up to 5 in-progress, non-dismissed stories you've actually opened, most-recently-opened first (`read_state.last_opened_at`) — click to jump straight back into the Reader at your last chapter.

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

Persisted to `localStorage` (`readerPrefs.ts`), applied live via inline styles on the prose element: font family (Serif/Sans toggle, or a **custom font name** typed in — e.g. "Bookerly" — used if installed locally, falling back to the Serif/Sans choice otherwise; nothing is downloaded/bundled), size (14–28px), weight (Light/Regular/Bold), line height (1.2–2.4), letter-spacing/kerning (−0.05 to 0.2em), and page width (40–90ch). Laid out as a 2-column grid (label | control) with controls stretched to fill the column, not left-aligned with trailing dead space.

## 7. Decision trail

Full detail, research sources, and the review rounds behind each locked decision live in the closed tickets under `wayfinder/tickets/` (map: `wayfinder/map.md`). That trail covers the map's original scope; everything past §6's Search FFN modal onward (theme toggle, reader typography, discovery/fandom-index) was designed and built directly in conversation after the map closed, without its own tickets.

## 8. Multi-source planning: Archive of Our Own (not yet built)

Everything above is FFN-specific by construction — the schema (`ffn_story_id`, `fandom` as a single URL-derived string), the fixed 9-category browse taxonomy, and the whole Cloudflare/webview fetch strategy (§3) all assume one source. Adding AO3 (archiveofourown.org) as a second source is a real architectural change, not a second scraper bolted on. This section is a plan, not a build — nothing below is implemented yet.

**Fetch strategy is genuinely different, confirmed by live test (2026-07-18):** a bare HTTP request (`urllib`, default headers, no cookies, no JS) against AO3's homepage, a tag-scoped work listing, and a full Mature-rated work page all returned real HTML with no Cloudflare challenge, no captcha, no block page. This is the opposite of FFN's situation (§3) — AO3 doesn't appear to run the same bot-fingerprinting defense. Practically: AO3 support could use the `reqwest` dependency that's been sitting unused since v1 (§2), skipping the webview/window-management machinery entirely for this source. **Not yet verified**: whether Explicit-rated works hit a login/age-confirmation gate when fetched without an authenticated session — only a Mature-rated work was tested live; this needs its own check before assuming full-catalog parity.

**robots.txt scope** (fetched live): disallows the *global* `/works?` search, `/autocomplete/`, `/downloads/`, `/external_works/`, and any `*/search?` endpoint for all crawlers, and blocks AI-training bots (GPTBot/CCBot/ChatGPT-User) wholesale. It does **not** disallow tag-scoped browsing (`/tags/{fandom}/works?...` — the shape this app would actually use, same as FFN's fandom-scoped browse pages) or individual work pages (`/works/{id}`). Any AO3 support should stay inside that boundary — same personal-use ToS posture already scoped for FFN (§1).

**Data model gap**: AO3's tagging is richer than what the current schema holds. Confirmed live markup shows, per work: rating, one or more **warnings** (multi-select, e.g. "Major Character Death, Graphic Depictions Of Violence" — FFN has none of this), a **category** (M/M, F/F, Gen, etc. — FFN has none of this), separate **relationship** and **character** tag groups (FFN's `tags` column mushes character/pairing into one free-text string), and arbitrary **freeform** tags on top of all that. `stories.rating`/`stories.tags` as they exist can't hold this without lossy squashing. Options to weigh later: (a) a `source` column plus AO3-specific nullable columns bolted onto `stories` (cheap, keeps one table, mirrors how `ffn_updated_at` was added), or (b) a proper per-source tag table (more correct, more migration work for zero current benefit since there's exactly one source today). Leaning (a) until there's an actual second source to prove the shape is needed — no sense generalizing against a hypothetical.

**Markup skeleton** (confirmed, for whenever a parser gets written): listing rows are `<li class="work blurb group">` — title+author in `h4.heading` (`a[href^="/works/"]` then `a[rel="author"]`), fandom in `h5.fandoms.heading a.tag`, rating/warning/category/complete-status as **class names** on `ul.required-tags li span` (e.g. `rating-mature`, `warning-yes`, `category-slash`, `complete-yes`) rather than label:value text the way FFN's metadata line works, relationship/character/freeform tags in `ul.tags.commas li.{relationships,characters,freeforms}`, and stats (words/chapters/kudos/comments/bookmarks/hits) in `dl.stats dd.{words,chapters,kudos,...}`. Work pages wrap everything in `#workskin` (`h2.title.heading`, `h3.byline.heading`).

**Locked**: sources are *not* merged into one popularity ranking. FFN's "Favs" and AO3's engagement metrics aren't the same signal at all — AO3's kudos is a one-click, near-frictionless "like" (315,868 kudos vs. only 56,015 bookmarks on the same test work), nothing like FFN's deliberate Favorite action. AO3's **bookmarks** is the closer analog to FFN's Favorites (both a deliberate "save/track this" action), not kudos — but even that likely isn't a fair apples-to-apples scale given the platforms' different audience sizes. Each source gets its own filter/badge and sorts within itself; "sort by favorites" across a mixed list isn't a feature that should exist.

**Open questions before any of this gets built**: whether "fandom" stays a single string or needs to become a proper set (AO3 crossovers list multiple fandoms natively, unlike FFN's one-fandom-per-story model); and whether the existing fandom-picker/category-browse UX (built entirely around FFN's fixed category tree) even makes sense for AO3's freeform tag space.
