# Graph Report - src-tauri  (2026-10-07)

## Corpus Check
- 18 files · ~27,889 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 423 nodes · 1111 edges · 17 communities (16 shown, 1 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 17 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `90330ba9`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Ffn
- fetch.rs
- queries.rs
- db.rs
- fetch_session.rs
- ffn.rs
- dispatch.rs
- ao3.rs
- tauri.conf.json
- Db
- fetch-page.json
- migrations.rs
- default.json
- CLAUDE.md

## God Nodes (most connected - your core abstractions)
1. `Db` - 38 edges
2. `FetchSession` - 21 edges
3. `run()` - 17 edges
4. `Ffn` - 17 edges
5. `test_db()` - 16 edges
6. `fetch_ffn()` - 16 edges
7. `Ao3` - 16 edges
8. `FetchOutcome` - 15 edges
9. `schema_conn()` - 14 edges
10. `persist_ffn_page()` - 12 edges

## Surprising Connections (you probably didn't know these)
- `join_tag_texts()` --calls--> `text_of()`  [INFERRED]
  src/ao3.rs → src/scrape.rs
- `parse_work_page()` --calls--> `parse_int()`  [INFERRED]
  src/ao3.rs → src/scrape.rs
- `parse_work_page()` --calls--> `text_of()`  [INFERRED]
  src/ao3.rs → src/scrape.rs
- `parse_listing_page()` --calls--> `parse_int()`  [INFERRED]
  src/ao3.rs → src/scrape.rs
- `parse_listing_page()` --calls--> `text_of()`  [INFERRED]
  src/ao3.rs → src/scrape.rs

## Import Cycles
- None detected.

## Communities (17 total, 1 thin omitted)

### Community 0 - "Ffn"
Cohesion: 0.09
Nodes (20): Metadata, ChapterRef, Ao3, ao3::Ao3ChapterRef, ao3_chapter_locator_from_url_propagates_malformed_error(), Ao3ChapterLocator, Ao3ListingLocator, ChapterInfo (+12 more)

### Community 1 - "fetch.rs"
Cohesion: 0.11
Nodes (43): Default, MutexGuard, S, BrowseFetched, ChapterFetched, classify_page(), FandomIndexFetched, FetchOutcome (+35 more)

### Community 2 - "queries.rs"
Cohesion: 0.13
Nodes (45): Row, bookmarks_scope_returns_only_bookmarked_stories_under_every_filter(), chapter_list_item_exposes_source_chapter_id(), ChapterContent, ChapterListItem, continue_reading_lists_unfinished_opened_stories_newest_first_and_honours_limit(), filters(), fold() (+37 more)

### Community 3 - "db.rs"
Cohesion: 0.12
Nodes (44): BTreeSet, FnOnce, ao3_browse_page_resume_starts_at_one_and_advances(), chapter_fetch_target(), chapter_upserts_carry_source_chapter_id_for_ao3_and_stay_null_for_ffn(), ensure_read_state(), FandomIndexEntry, ffn_and_ao3_stories_with_same_source_story_id_coexist() (+36 more)

### Community 4 - "fetch_session.rs"
Cohesion: 0.14
Nodes (28): Duration, F, Output, Receiver, Self, Sender, a_category_index_report_matches_by_category(), a_newer_start_rejects_the_older_fetch_and_leaves_the_new_one_pending() (+20 more)

### Community 5 - "ffn.rs"
Cohesion: 0.16
Nodes (30): build_browse_url(), category_from_index_url(), extract_labeled(), extract_trailing_tags(), ffn_locator_from_url(), ffn_locator_from_url_malformed_for_non_numeric_chapter_num(), ffn_locator_from_url_malformed_for_non_numeric_story_id(), ffn_locator_from_url_matches_a_real_chapter_url() (+22 more)

### Community 6 - "dispatch.rs"
Cohesion: 0.11
Nodes (19): ao3_fandom_override_applies_on_listing_rows(), ao3_fandom_override_is_ignored_on_the_chapter_path(), category_index_page_with_no_fandoms_is_an_error(), chapter_fetch_url(), chapter_fetch_url_builds_the_ffn_chapter_url_from_the_stored_story(), chapter_fetch_url_rejects_an_unknown_story_before_any_fetch(), chapter_fetch_url_uses_the_recorded_ao3_chapter_id_and_falls_back_to_the_bare_work(), chapter_page_failing_mid_toc_leaves_no_story_behind() (+11 more)

### Community 7 - "ao3.rs"
Cohesion: 0.16
Nodes (27): Selector, ao3_locator_from_url(), ao3_locator_from_url_does_not_false_positive_on_a_listing_url(), ao3_locator_from_url_malformed_for_non_numeric_chapter_id(), ao3_locator_from_url_malformed_for_non_numeric_work_id(), ao3_locator_from_url_matches_a_one_shot_url(), ao3_locator_from_url_matches_a_real_chapter_url(), ao3_locator_from_url_no_match_for_a_user_profile_url() (+19 more)

### Community 8 - "tauri.conf.json"
Cohesion: 0.08
Nodes (23): icons/128x128@2x.png, icons/128x128.png, icons/32x32.png, icons/icon.icns, icons/icon.ico, app, security, windows (+15 more)

### Community 9 - "Db"
Cohesion: 0.38
Nodes (23): Db, Mutex, db_health_check(), get_chapter(), get_continue_reading(), get_story(), list_bookmarked_fandoms(), list_bookmarks() (+15 more)

### Community 10 - "fetch-page.json"
Cohesion: 0.15
Nodes (12): description, identifier, core:default, permissions, remote, urls, $schema, windows (+4 more)

### Community 11 - "migrations.rs"
Cohesion: 0.36
Nodes (12): add_column_if_missing(), corrupt_stories_fk(), dangling_stories_fk_count(), fix_ao3_landmark_heading_in_chapters(), fix_dangling_stories_fk(), fix_dangling_stories_fk_is_noop_on_healthy_db(), fix_dangling_stories_fk_repairs_corrupted_db_and_preserves_data(), fix_percent_encoded_fandoms() (+4 more)

### Community 12 - "default.json"
Cohesion: 0.20
Nodes (9): description, identifier, core:default, permissions, $schema, windows, allow-main-commands, main (+1 more)

## Knowledge Gaps
- **35 isolated node(s):** `$schema`, `identifier`, `description`, `main`, `core:default` (+30 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Db` connect `Db` to `fetch.rs`, `db.rs`, `dispatch.rs`?**
  _High betweenness centrality (0.251) - this node is a cross-community bridge._
- **Why does `StoryMetadata` connect `ffn.rs` to `queries.rs`, `db.rs`?**
  _High betweenness centrality (0.065) - this node is a cross-community bridge._
- **Why does `Ao3StoryMetadata` connect `ao3.rs` to `queries.rs`, `db.rs`?**
  _High betweenness centrality (0.060) - this node is a cross-community bridge._
- **What connects `$schema`, `identifier`, `description` to the rest of the system?**
  _35 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Ffn` be split into smaller, more focused modules?**
  _Cohesion score 0.08636363636363636 - nodes in this community are weakly interconnected._
- **Should `fetch.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.11248185776487664 - nodes in this community are weakly interconnected._
- **Should `queries.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.12943262411347517 - nodes in this community are weakly interconnected._