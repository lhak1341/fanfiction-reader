# Graph Report - .  (2026-08-05)

## Corpus Check
- 66 files · ~50,334 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 664 nodes · 1328 edges · 54 communities (29 shown, 25 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 22 edges (avg confidence: 0.77)
- Token cost: 133,787 input · 0 output

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
- Bookmarks Screen & Chapter Cache
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
- Search Screen State
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

## God Nodes (most connected - your core abstractions)
1. `$lib/api` - 54 edges
2. `Db` - 33 edges
3. `$lib/types` - 30 edges
4. `run()` - 20 edges
5. `$lib/components/Reader.svelte` - 20 edges
6. `test_db()` - 18 edges
7. `Ao3` - 17 edges
8. `Ffn` - 16 edges
9. `$lib/searchScreen.svelte` - 14 edges
10. `emit_fetch_outcome()` - 13 edges

## Surprising Connections (you probably didn't know these)
- `README.md — Tauri + SvelteKit + TypeScript template` --semantically_similar_to--> `SPEC.md — Longform App Spec`  [INFERRED] [semantically similar]
  README.md → docs/SPEC.md
- `README.md — Tauri + SvelteKit + TypeScript template` --semantically_similar_to--> `Decision: Svelte + Bun frontend`  [INFERRED] [semantically similar]
  README.md → docs/wayfinder/tickets/002-frontend-framework-rust-crates.md
- `app.html — SvelteKit shell template` --shares_data_with--> `Decision: Svelte + Bun frontend`  [INFERRED]
  src/app.html → docs/wayfinder/tickets/002-frontend-framework-rust-crates.md
- `parse_story_page()` --calls--> `parse_int()`  [INFERRED]
  src-tauri/src/ffn.rs → src-tauri/src/scrape.rs
- `parse_story_page()` --calls--> `text_of()`  [INFERRED]
  src-tauri/src/ffn.rs → src-tauri/src/scrape.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **FicSource seam — dual Rust/TS implementation across FFN and AO3** — docs_spec_ficsource_trait, context_ficsource, docs_spec_ao3_rs, docs_spec_ficsource_ts [INFERRED 0.85]
- **Wayfinder tickets feeding the locked SPEC decisions** — docs_wayfinder_map_overview, docs_wayfinder_tickets_001_cloudflare_session_handling_overview, docs_wayfinder_tickets_002_frontend_framework_rust_crates_overview, docs_wayfinder_tickets_003_data_model_sqlite_schema_overview, docs_wayfinder_tickets_004_fanficfare_port_scope_overview, docs_wayfinder_tickets_005_ui_ux_layout_prototype_overview [EXTRACTED 1.00]
- **Longform's six-table SQLite schema** — docs_spec_stories_table, docs_spec_chapters_table, docs_spec_read_state_table, docs_spec_filter_presets_table, docs_spec_browse_state_table, docs_spec_fandom_index_table [EXTRACTED 1.00]

## Communities (54 total, 25 thin omitted)

### Community 0 - "Chapter/Listing Data Models"
Cohesion: 0.09
Nodes (76): ChapterContent, ChapterListItem, FandomIndexEntry, Db, ao3_fandom_override_applies_on_listing_rows(), ao3_fandom_override_is_ignored_on_the_chapter_path(), BrowseFetched, BrowseFetchError (+68 more)

### Community 1 - "Source Locator Abstraction"
Cohesion: 0.09
Nodes (20): ChapterLocator, Metadata, ChapterRef, Ao3, ao3::Ao3ChapterRef, ao3_chapter_locator_from_url_propagates_malformed_error(), Ao3ChapterLocator, Ao3ListingLocator (+12 more)

### Community 2 - "FicSource Architecture Docs"
Cohesion: 0.05
Nodes (48): CLAUDE.md — Longform dev practices & gotchas, Ffn / Ao3 impls (source.rs), ffnAdapter / ao3Adapter (ficSource.ts), FicSource seam, Why: FicSource deliberately excludes DB upsert / category-index / search UI, ao3.rs (parser), Why: AO3 has no TLS fingerprinting, bare reqwest works with a real UA header, build_tag_browse_url (ao3.rs) (+40 more)

### Community 3 - "DB Migration & Schema Logic"
Cohesion: 0.12
Nodes (45): FnOnce, add_column_if_missing(), ao3_browse_page_resume_starts_at_one_and_advances(), chapter_upserts_carry_source_chapter_id_for_ao3_and_stay_null_for_ffn(), corrupt_stories_fk(), dangling_stories_fk_count(), ensure_read_state(), FandomIndexEntry (+37 more)

### Community 4 - "Reader Component Logic"
Cohesion: 0.06
Nodes (35): $lib/components/Reader.svelte, displayedHtml, fontFamilyCss, goToChapter(), handleKeydown(), handleScroll(), navCenterIdx, navHover (+27 more)

### Community 5 - "Package Dependencies"
Cohesion: 0.05
Nodes (39): dependencies, lucide-svelte, @tauri-apps/api, @tauri-apps/plugin-opener, description, devDependencies, svelte, svelte-check (+31 more)

### Community 6 - "Query Layer (queries.rs)"
Cohesion: 0.18
Nodes (32): Row, chapter_list_item_exposes_source_chapter_id(), ChapterContent, ChapterListItem, fold(), get_chapter(), get_continue_reading(), get_story() (+24 more)

### Community 7 - "AO3 URL Locator Parsing"
Cohesion: 0.13
Nodes (31): Selector, ao3_locator_from_url(), ao3_locator_from_url_does_not_false_positive_on_a_listing_url(), ao3_locator_from_url_malformed_for_non_numeric_chapter_id(), ao3_locator_from_url_malformed_for_non_numeric_work_id(), ao3_locator_from_url_matches_a_one_shot_url(), ao3_locator_from_url_matches_a_real_chapter_url(), ao3_locator_from_url_no_match_for_a_user_profile_url() (+23 more)

### Community 8 - "Frontend API/Event Bindings"
Cohesion: 0.09
Nodes (14): $lib/api, browseEvents, chapterEvents, fandomIndexEvents, fetchChapterAndWait(), indexAllCategoriesAndWait(), indexFfnCategory(), indexFfnCategoryAndWait() (+6 more)

### Community 9 - "FFN Parsing & Scraping"
Cohesion: 0.19
Nodes (26): build_browse_url(), category_from_index_url(), extract_labeled(), extract_trailing_tags(), ffn_locator_from_url(), ffn_locator_from_url_malformed_for_non_numeric_chapter_num(), ffn_locator_from_url_malformed_for_non_numeric_story_id(), ffn_locator_from_url_matches_a_real_chapter_url() (+18 more)

### Community 10 - "App Icons & Tauri Config"
Cohesion: 0.08
Nodes (23): icons/128x128@2x.png, icons/128x128.png, icons/32x32.png, icons/icon.icns, icons/icon.ico, app, security, windows (+15 more)

### Community 11 - "UI List/Search Components"
Cohesion: 0.13
Nodes (16): @tauri-apps/api/core, $lib/components/RowActions.svelte, spinning, $lib/components/SearchModal.svelte, $lib/components/StatusBadges.svelte, $lib/components/StoryList.svelte, shortDate(), toShort() (+8 more)

### Community 12 - "Bookmarks Screen & Chapter Cache"
Cohesion: 0.30
Nodes (12): $lib/bookmarksScreen.svelte, ensureChapterCached(), EnsureChapterCachedResult, ficSourceAdapter, $lib/pacedLoop, DEFAULT_JITTER_MS, jitterDelay(), runPacedBestEffort() (+4 more)

### Community 13 - "Fandom & Search State Concepts"
Cohesion: 0.14
Nodes (14): Fandom (stories.fandom), AO3 tag map (fandom.ts), lastSearchSource (+page.svelte), Why: no cross-Source fandom-identity table, rename_fandom (queries.rs), Bookmarks screen, ensureChapterCached (chapterCache.ts), fandom_override param (search_ao3 / try_persist_ao3_fetch) (+6 more)

### Community 14 - "Fetch-Page Tauri Capability"
Cohesion: 0.15
Nodes (12): allow-report-fetch-result, fetch-page, https://m.fanfiction.net/*, https://www.fanfiction.net/*, description, identifier, core:default, permissions (+4 more)

### Community 15 - "TypeScript Config"
Cohesion: 0.15
Nodes (12): ./.svelte-kit/tsconfig.json, compilerOptions, allowJs, checkJs, esModuleInterop, forceConsistentCasingInFileNames, moduleResolution, resolveJsonModule (+4 more)

### Community 16 - "Fandom Tag Mapping & FicSource"
Cohesion: 0.27
Nodes (8): applyFandomRename(), getAo3Tag(), tagMapPref, ao3Adapter, ffnAdapter, $lib/searchScreen.svelte, BrowseFetched, StoryStatus

### Community 17 - "Search Modal Screen State"
Cohesion: 0.27
Nodes (5): saveAo3Tag(), SearchModalScreen, FandomIndexEntry, FFN_CATEGORIES, StorySource

### Community 18 - "Default Tauri Capability"
Cohesion: 0.20
Nodes (9): allow-main-commands, main, opener:default, description, identifier, core:default, permissions, $schema (+1 more)

### Community 20 - "Story Flag Actions"
Cohesion: 0.33
Nodes (9): $lib/storyFlagActions, MESSAGES, toggleAndRefresh(), toggleBookmarkedAndRefresh(), toggleDismissedAndRefresh(), toggleReadAndRefresh(), StoryFlag, TOGGLE (+1 more)

### Community 22 - "Read/Bookmark Flag Schema"
Cohesion: 0.29
Nodes (8): read_state table, Bookmark flag, Dismiss ('not for me') flag, Manual mark-as-read (not automatic), Amendment 2: bookmarked / bookmarked_at, Amendment 3: last_opened_at / Continue Reading, Amendment: dismissed / dismissed_at, Locked UI layout: Search/Bookmarks tabs + immersive Reader

### Community 25 - "Story Flag Reconciliation"
Cohesion: 0.67
Nodes (4): Story flag (isRead/dismissed/bookmarked), Why: toggleStoryFlag stays pure, callers own reconciliation, storyFlagActions.ts, toggleStoryFlag (storyFlags.ts)

## Knowledge Gaps
- **150 isolated node(s):** `name`, `version`, `description`, `type`, `dev` (+145 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **25 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Db` connect `Chapter/Listing Data Models` to `DB Migration & Schema Logic`?**
  _High betweenness centrality (0.060) - this node is a cross-community bridge._
- **Why does `StoryMetadata` connect `FFN Parsing & Scraping` to `DB Migration & Schema Logic`, `Query Layer (queries.rs)`?**
  _High betweenness centrality (0.036) - this node is a cross-community bridge._
- **Why does `Ao3StoryMetadata` connect `AO3 URL Locator Parsing` to `DB Migration & Schema Logic`, `Query Layer (queries.rs)`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **What connects `name`, `version`, `description` to the rest of the system?**
  _150 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Chapter/Listing Data Models` be split into smaller, more focused modules?**
  _Cohesion score 0.08672086720867209 - nodes in this community are weakly interconnected._
- **Should `Source Locator Abstraction` be split into smaller, more focused modules?**
  _Cohesion score 0.08636363636363636 - nodes in this community are weakly interconnected._
- **Should `FicSource Architecture Docs` be split into smaller, more focused modules?**
  _Cohesion score 0.04609929078014184 - nodes in this community are weakly interconnected._