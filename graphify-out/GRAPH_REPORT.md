# Graph Report - fanfiction-reader  (2026-10-08)

## Corpus Check
- 75 files · ~63,367 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 889 nodes · 1724 edges · 137 communities (30 shown, 107 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 18 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `83e4f651`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Chapter/Listing Data Models
- Source Locator Abstraction
- FicSource Architecture Docs
- DB Migration & Schema Logic
- Reader Component Logic
- Package Dependencies
- Query Layer (queries.rs)
- AO3 URL Locator Parsing
- Frontend API/Event Bindings
- FFN Parsing & Scraping
- App Icons & Tauri Config
- UI List/Search Components
- Fandom & Search State Concepts
- Fetch-Page Tauri Capability
- TypeScript Config
- Fandom Tag Mapping & FicSource
- Search Modal Screen State
- Default Tauri Capability
- Reader Screen State
- Story Flag Actions
- Bookmarks Screen State
- Read/Bookmark Flag Schema
- Fandom Picker Component
- Story Flag Reconciliation
- AO3 No-Merge Ranking Decision
- Blocked-Page Detection Fix
- Local-Only Solo App Decision
- Root Layout & Global CSS
- Svelte Config
- Project Overview Concept
- Source Concept
- AO3 Browse State Table
- Correlated Fetch Event Waiting
- Browse State Table
- Browse URL Builder
- Chapters Table Schema
- Fandom Index Table Schema
- FicSource TS Interface
- Lucide Icon Dependency
- Fandom Index Parser
- rusqlite Crate Dependency
- serde Crate Dependency
- Tauri Crate Dependency
- url Crate Dependency
- Mobile-to-WWW URL Rewrite Fix
- fetch.rs
- +page.svelte
- Longform
- Tauri + SvelteKit + TypeScript
- CLAUDE.md
- CLAUDE.md — Longform dev practices & gotchas
- Fandom (stories.fandom)
- AO3 tag map (fandom.ts)
- Ffn / Ao3 impls (source.rs)
- ffnAdapter / ao3Adapter (ficSource.ts)
- FicSource seam
- Why: FicSource deliberately excludes DB upsert / category-index / search UI
- lastSearchSource (+page.svelte)
- Why: no cross-Source fandom-identity table
- rename_fandom (queries.rs)
- Why: toggleStoryFlag stays pure, callers own reconciliation
- storyFlagActions.ts
- toggleStoryFlag (storyFlags.ts)
- Why: FFN Favs vs AO3 kudos/bookmarks aren't the same signal
- ao3.rs (parser)
- Why: AO3 has no TLS fingerprinting, bare reqwest works with a real UA header
- Bookmarks screen
- build_tag_browse_url (ao3.rs)
- classify_fetch_target (lib.rs)
- command_registration.rs test
- migrate() (db.rs)
- ensureChapterCached (chapterCache.ts)
- Bug: extract_trailing_tags leaked '- Complete' into tags
- fandom_override param (search_ao3 / try_persist_ao3_fetch)
- FandomPicker.svelte
- fetch_ao3_html (lib.rs)
- fetch_page command (lib.rs)
- FetchLock (tokio Mutex, renamed PendingFetch)
- FetchOutcome / FfnFetchOutcome (lib.rs)
- FicSource trait (source.rs)
- filter_presets table (unused)
- fix_dangling_stories_fk migration (bug fix)
- migrate_stories_source_column (db.rs)
- SPEC.md — Longform App Spec
- parse_browse_page (ffn.rs)
- parse_chapter_content (ffn.rs)
- parse_story_page (ffn.rs)
- Reader screen
- report_fetch_result command
- reqwest crate
- scraper crate (HTML parsing)
- search_ao3 command
- SearchModal.svelte
- Search screen
- stories table
- try_persist_ao3_fetch (lib.rs)
- try_persist_fetch::<S: FicSource> (lib.rs)
- type_mirror.rs test
- upsert_ao3_story (db.rs)
- Bookmark flag
- Dismiss ('not for me') flag
- Manual mark-as-read (not automatic)
- wayfinder map.md — FFN Reader App Spec charting doc
- Why: solo/local-only caps legal exposure, avoids auth/multi-user
- Ticket 001 — Cloudflare/session-handling strategy
- Ticket 002 — Frontend framework & Rust crates
- Ticket 003 — Data model / SQLite schema
- Ticket 004 — FanFicFare parsing-logic port scope
- Ticket 005 — UI/UX layout prototype
- README.md — Tauri + SvelteKit + TypeScript template
- Blocked-page detection scoped to <title> only
- FanFicFare/reqwest Cloudflare GitHub issues (622, 703, 614, reqwest#2227)
- Why: bare reqwest/curl fail Cloudflare TLS fingerprinting even w/ cookie; m. subdomain gets no lighter protection
- Decision: fetch via Tauri WebView, not bare HTTP client
- Decision: tauri/scraper/rusqlite/serde/reqwest core crates
- Amendment 2: bookmarked / bookmarked_at
- Amendment 3: last_opened_at / Continue Reading
- Amendment: dismissed / dismissed_at
- Original 4-table schema (stories/chapters/read_state/filter_presets)
- FanFicFare (github.com/JimmXinu/FanFicFare)
- Port target: adapter_fanfictionnet.py (FanFictionNetSiteAdapter)
- Locked UI layout: Search/Bookmarks tabs + immersive Reader
- FETCH_TESTING.md
- TESTING.md
- deploy-local.sh
- CLAUDE.md
- CLAUDE.md
- fetch_session.rs
- BookmarksScreen

## God Nodes (most connected - your core abstractions)
1. `Db` - 40 edges
2. `$lib/api` - 38 edges
3. `FetchSession` - 28 edges
4. `$lib/types` - 28 edges
5. `$lib/components/Reader.svelte` - 24 edges
6. `FetchOutcome` - 17 edges
7. `fetch_ffn_via()` - 17 edges
8. `run()` - 17 edges
9. `Ffn` - 17 edges
10. `test_db()` - 16 edges

## Surprising Connections (you probably didn't know these)
- `app.html — SvelteKit shell template` --shares_data_with--> `Decision: Svelte + Bun frontend`  [INFERRED]
  src/app.html → docs/wayfinder/tickets/002-frontend-framework-rust-crates.md
- `count()` --references--> `Db`  [EXTRACTED]
  src-tauri/src/dispatch.rs → src-tauri/src/db.rs
- `parse_story_page()` --calls--> `parse_int()`  [INFERRED]
  src-tauri/src/ffn.rs → src-tauri/src/scrape.rs
- `parse_story_page()` --calls--> `text_of()`  [INFERRED]
  src-tauri/src/ffn.rs → src-tauri/src/scrape.rs
- `parse_fandom_index()` --calls--> `text_of()`  [INFERRED]
  src-tauri/src/ffn.rs → src-tauri/src/scrape.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **FicSource seam — dual Rust/TS implementation across FFN and AO3** — docs_spec_ficsource_trait, context_ficsource, docs_spec_ao3_rs, docs_spec_ficsource_ts [INFERRED 0.85]
- **Wayfinder tickets feeding the locked SPEC decisions** — docs_wayfinder_map_overview, docs_wayfinder_tickets_001_cloudflare_session_handling_overview, docs_wayfinder_tickets_002_frontend_framework_rust_crates_overview, docs_wayfinder_tickets_003_data_model_sqlite_schema_overview, docs_wayfinder_tickets_004_fanficfare_port_scope_overview, docs_wayfinder_tickets_005_ui_ux_layout_prototype_overview [EXTRACTED 1.00]
- **Longform's six-table SQLite schema** — docs_spec_stories_table, docs_spec_chapters_table, docs_spec_read_state_table, docs_spec_filter_presets_table, docs_spec_browse_state_table, docs_spec_fandom_index_table [EXTRACTED 1.00]

## Communities (137 total, 107 thin omitted)

### Community 0 - "Chapter/Listing Data Models"
Cohesion: 0.28
Nodes (28): ChapterContent, ChapterListItem, FandomIndexEntry, Db, Mutex, db_health_check(), get_chapter(), get_continue_reading() (+20 more)

### Community 1 - "Source Locator Abstraction"
Cohesion: 0.08
Nodes (21): Metadata, ChapterRef, Ao3, ao3::Ao3ChapterRef, ao3_chapter_locator_from_url_propagates_malformed_error(), Ao3ChapterLocator, Ao3ListingLocator, ChapterInfo (+13 more)

### Community 3 - "DB Migration & Schema Logic"
Cohesion: 0.09
Nodes (56): BTreeSet, FnOnce, ao3_browse_page_resume_starts_at_one_and_advances(), chapter_fetch_target(), chapter_upserts_carry_source_chapter_id_for_ao3_and_stay_null_for_ffn(), ensure_read_state(), FandomIndexEntry, ffn_and_ao3_stories_with_same_source_story_id_coexist() (+48 more)

### Community 4 - "Reader Component Logic"
Cohesion: 0.05
Nodes (38): $lib/components/Reader.svelte, displayedHtml, drawerOpen, fontFamilyCss, fontPanelOpen, goToChapter(), handleKeydown(), handleScroll() (+30 more)

### Community 5 - "Package Dependencies"
Cohesion: 0.04
Nodes (45): jsdom, dependencies, lucide-svelte, @tauri-apps/api, @tauri-apps/plugin-opener, description, devDependencies, jsdom (+37 more)

### Community 6 - "Query Layer (queries.rs)"
Cohesion: 0.13
Nodes (45): Row, bookmarks_scope_returns_only_bookmarked_stories_under_every_filter(), chapter_list_item_exposes_source_chapter_id(), ChapterContent, ChapterListItem, continue_reading_lists_unfinished_opened_stories_newest_first_and_honours_limit(), filters(), fold() (+37 more)

### Community 7 - "AO3 URL Locator Parsing"
Cohesion: 0.13
Nodes (32): Selector, ao3_locator_from_url(), ao3_locator_from_url_does_not_false_positive_on_a_listing_url(), ao3_locator_from_url_malformed_for_non_numeric_chapter_id(), ao3_locator_from_url_malformed_for_non_numeric_work_id(), ao3_locator_from_url_matches_a_one_shot_url(), ao3_locator_from_url_matches_a_real_chapter_url(), ao3_locator_from_url_no_match_for_a_user_profile_url() (+24 more)

### Community 8 - "Frontend API/Event Bindings"
Cohesion: 0.09
Nodes (11): @tauri-apps/api/core, $lib/api, ensureChapterCached(), EnsureChapterCachedResult, $lib/types, ChapterContent, ChapterFetched, ChapterListItem (+3 more)

### Community 9 - "FFN Parsing & Scraping"
Cohesion: 0.18
Nodes (27): build_browse_url(), category_from_index_url(), extract_labeled(), extract_trailing_tags(), ffn_locator_from_url(), ffn_locator_from_url_malformed_for_non_numeric_chapter_num(), ffn_locator_from_url_malformed_for_non_numeric_story_id(), ffn_locator_from_url_matches_a_real_chapter_url() (+19 more)

### Community 10 - "App Icons & Tauri Config"
Cohesion: 0.08
Nodes (23): icons/128x128@2x.png, icons/128x128.png, icons/32x32.png, icons/icon.icns, icons/icon.ico, app, security, windows (+15 more)

### Community 11 - "UI List/Search Components"
Cohesion: 0.24
Nodes (7): $lib/components/RowActions.svelte, spinning, $lib/components/SearchModal.svelte, $lib/components/StatusBadges.svelte, $lib/components/StoryList.svelte, shortDate(), toShort()

### Community 13 - "Fandom & Search State Concepts"
Cohesion: 0.05
Nodes (41): 1. Scope, 2. Architecture, 3. Fetching & Cloudflare strategy, 4. Scraping, 5. Data model (SQLite), 6. UI/UX, 7. Decision trail, 8. Multi-source planning: Archive of Our Own (in progress) (+33 more)

### Community 14 - "Fetch-Page Tauri Capability"
Cohesion: 0.15
Nodes (12): allow-report-fetch-result, fetch-page, https://m.fanfiction.net/*, https://www.fanfiction.net/*, description, identifier, core:default, permissions (+4 more)

### Community 15 - "TypeScript Config"
Cohesion: 0.15
Nodes (12): ./.svelte-kit/tsconfig.json, compilerOptions, allowJs, checkJs, esModuleInterop, forceConsistentCasingInFileNames, moduleResolution, resolveJsonModule (+4 more)

### Community 16 - "Fandom Tag Mapping & FicSource"
Cohesion: 0.16
Nodes (12): applyFandomRename(), getAo3Tag(), renameAo3Tag(), tagMapPref, ao3Adapter, ffnAdapter, ficSourceAdapter, $lib/searchScreen.svelte (+4 more)

### Community 17 - "Search Modal Screen State"
Cohesion: 0.24
Nodes (6): saveAo3Tag(), SearchModalScreen, SearchMode, FandomIndexEntry, FFN_CATEGORIES, StorySource

### Community 18 - "Default Tauri Capability"
Cohesion: 0.20
Nodes (9): allow-main-commands, main, opener:default, description, identifier, core:default, permissions, $schema (+1 more)

### Community 20 - "Story Flag Actions"
Cohesion: 0.33
Nodes (9): $lib/storyFlagActions, MESSAGES, toggleAndRefresh(), toggleBookmarkedAndRefresh(), toggleDismissedAndRefresh(), toggleReadAndRefresh(), StoryFlag, TOGGLE (+1 more)

### Community 21 - "Bookmarks Screen State"
Cohesion: 0.08
Nodes (32): S, ao3_fandom_override_applies_on_listing_rows(), ao3_fandom_override_is_ignored_on_the_chapter_path(), BrowseFetched, category_index_page_with_no_fandoms_is_an_error(), chapter_fetch_url(), chapter_fetch_url_builds_the_ffn_chapter_url_from_the_stored_story(), chapter_fetch_url_rejects_an_unknown_story_before_any_fetch() (+24 more)

### Community 54 - "fetch.rs"
Cohesion: 0.11
Nodes (50): Arc, BrowseFetched, ChapterFetched, FandomIndexFetched, add_story_by_url(), challenge(), dump_last_fetch(), FakeWebview (+42 more)

### Community 137 - "fetch_session.rs"
Cohesion: 0.11
Nodes (40): Default, F, MutexGuard, Output, Receiver, Sender, a_background_start_never_supersedes_a_pending_foreground_fetch(), a_background_start_proceeds_once_the_foreground_fetch_has_finished() (+32 more)

### Community 140 - "BookmarksScreen"
Cohesion: 0.11
Nodes (15): $lib/bookmarksScreen.svelte, BookmarksScreen, $lib/pacedLoop, DEFAULT_JITTER_MS, DEFAULT_RETRY_JITTER_MS, describeFailures(), jitterDelay(), PacedFailure (+7 more)

## Knowledge Gaps
- **244 isolated node(s):** `name`, `version`, `description`, `type`, `dev` (+239 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **107 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Db` connect `Chapter/Listing Data Models` to `DB Migration & Schema Logic`, `Bookmarks Screen State`, `fetch.rs`?**
  _High betweenness centrality (0.060) - this node is a cross-community bridge._
- **Why does `StoryMetadata` connect `FFN Parsing & Scraping` to `DB Migration & Schema Logic`, `Query Layer (queries.rs)`?**
  _High betweenness centrality (0.039) - this node is a cross-community bridge._
- **Why does `sample_ffn()` connect `Query Layer (queries.rs)` to `FFN Parsing & Scraping`?**
  _High betweenness centrality (0.032) - this node is a cross-community bridge._
- **What connects `name`, `version`, `description` to the rest of the system?**
  _244 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Source Locator Abstraction` be split into smaller, more focused modules?**
  _Cohesion score 0.07909604519774012 - nodes in this community are weakly interconnected._
- **Should `DB Migration & Schema Logic` be split into smaller, more focused modules?**
  _Cohesion score 0.09265536723163842 - nodes in this community are weakly interconnected._
- **Should `Reader Component Logic` be split into smaller, more focused modules?**
  _Cohesion score 0.051418439716312055 - nodes in this community are weakly interconnected._