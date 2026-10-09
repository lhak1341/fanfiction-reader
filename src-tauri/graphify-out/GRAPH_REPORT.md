# Graph Report - src-tauri  (2026-10-09)

## Corpus Check
- 19 files · ~33,844 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 561 nodes · 1567 edges · 18 communities (17 shown, 1 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 25 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `d07ab814`
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
- fetch_env.rs

## God Nodes (most connected - your core abstractions)
1. `Db` - 52 edges
2. `FetchSession` - 35 edges
3. `fixtures()` - 25 edges
4. `test_db()` - 24 edges
5. `FakeEnv` - 23 edges
6. `block_on()` - 19 edges
7. `fetch_ffn_via()` - 17 edges
8. `add_story_by_url_via()` - 16 edges
9. `run_fetch()` - 16 edges
10. `run()` - 16 edges

## Surprising Connections (you probably didn't know these)
- `bookmarked()` --references--> `Db`  [EXTRACTED]
  src/fetch.rs → src/db.rs
- `story_count()` --references--> `Db`  [EXTRACTED]
  src/fetch.rs → src/db.rs
- `fix_dangling_stories_fk_is_noop_on_healthy_db()` --calls--> `temp_conn()`  [INFERRED]
  src/migrations.rs → src/db.rs
- `fix_dangling_stories_fk_repairs_corrupted_db_and_preserves_data()` --calls--> `temp_conn()`  [INFERRED]
  src/migrations.rs → src/db.rs
- `fetch_ao3_via()` --calls--> `get_with_retry()`  [INFERRED]
  src/fetch.rs → src/fetch_env.rs

## Import Cycles
- None detected.

## Communities (18 total, 1 thin omitted)

### Community 0 - "Ffn"
Cohesion: 0.07
Nodes (22): FromSqlResult, Metadata, ChapterRef, Ao3, ao3::Ao3ChapterRef, ao3_chapter_locator_from_url_propagates_malformed_error(), Ao3ChapterLocator, ChapterInfo (+14 more)

### Community 1 - "fetch.rs"
Cohesion: 0.10
Nodes (66): Default, MutexGuard, add_story_by_url(), add_story_by_url_does_not_bookmark_when_the_fetch_fails(), add_story_by_url_imports_and_bookmarks_an_ao3_story_through_the_http_path(), add_story_by_url_imports_and_bookmarks_an_ffn_story(), add_story_by_url_rejects_a_bad_url_before_any_fetch(), add_story_by_url_via() (+58 more)

### Community 2 - "queries.rs"
Cohesion: 0.09
Nodes (52): Error, Row, absent_or_null_filters_mean_no_filter_and_favorites(), bookmarks_scope_returns_only_bookmarked_stories_under_every_filter(), ChapterContent, ChapterListItem, continue_reading_lists_unfinished_opened_stories_newest_first_and_honours_limit(), filter_enums_deserialize_only_their_wire_spellings() (+44 more)

### Community 3 - "db.rs"
Cohesion: 0.12
Nodes (44): FnOnce, ao3_browse_page_resume_starts_at_one_and_advances(), chapter_fetch_target(), chapter_upserts_carry_source_chapter_id_for_ao3_and_stay_null_for_ffn(), ensure_read_state(), FandomIndexEntry, ffn_and_ao3_stories_with_same_source_story_id_coexist(), ffn_browse_page_resume_starts_at_one_and_advances() (+36 more)

### Community 4 - "fetch_session.rs"
Cohesion: 0.12
Nodes (36): Receiver, Sender, a_background_start_never_supersedes_a_pending_foreground_fetch(), a_background_start_proceeds_once_the_foreground_fetch_has_finished(), a_category_index_report_matches_by_category(), a_foreground_start_supersedes_a_pending_background_fetch(), a_newer_start_rejects_the_older_fetch_and_leaves_the_new_one_pending(), a_stale_report_for_the_superseded_target_is_dropped() (+28 more)

### Community 5 - "ffn.rs"
Cohesion: 0.17
Nodes (29): build_browse_url(), category_from_index_url(), extract_labeled(), extract_trailing_tags(), ffn_locator_from_url(), ffn_locator_from_url_malformed_for_non_numeric_chapter_num(), ffn_locator_from_url_malformed_for_non_numeric_story_id(), ffn_locator_from_url_matches_a_real_chapter_url() (+21 more)

### Community 6 - "dispatch.rs"
Cohesion: 0.07
Nodes (46): FromSql, S, ao3_fandom_override_applies_on_listing_rows(), ao3_fandom_override_is_ignored_on_the_chapter_path(), ao3_listing_fetch_with_no_requested_locator_does_not_record_browse_state(), ao3_listing_records_under_the_requested_tag_even_when_the_fetched_url_names_another(), bookmark_imported_story(), bookmark_imported_story_bookmarks_only_that_flag() (+38 more)

### Community 7 - "ao3.rs"
Cohesion: 0.13
Nodes (34): Selector, ao3_locator_from_url(), ao3_locator_from_url_does_not_false_positive_on_a_listing_url(), ao3_locator_from_url_malformed_for_non_numeric_chapter_id(), ao3_locator_from_url_malformed_for_non_numeric_work_id(), ao3_locator_from_url_matches_a_one_shot_url(), ao3_locator_from_url_matches_a_real_chapter_url(), ao3_locator_from_url_no_match_for_a_user_profile_url() (+26 more)

### Community 8 - "tauri.conf.json"
Cohesion: 0.08
Nodes (23): icons/128x128@2x.png, icons/128x128.png, icons/32x32.png, icons/icon.icns, icons/icon.ico, app, security, windows (+15 more)

### Community 9 - "Db"
Cohesion: 0.34
Nodes (24): Db, Mutex, count(), db_health_check(), get_continue_reading(), get_story(), list_bookmarked_fandoms(), list_bookmarks() (+16 more)

### Community 10 - "fetch-page.json"
Cohesion: 0.15
Nodes (12): description, identifier, core:default, permissions, remote, urls, $schema, windows (+4 more)

### Community 11 - "migrations.rs"
Cohesion: 0.33
Nodes (13): add_column_if_missing(), corrupt_stories_fk(), dangling_stories_fk_count(), fix_ao3_landmark_heading_in_chapters(), fix_dangling_stories_fk(), fix_dangling_stories_fk_is_noop_on_healthy_db(), fix_dangling_stories_fk_repairs_corrupted_db_and_preserves_data(), fix_percent_encoded_fandoms() (+5 more)

### Community 12 - "default.json"
Cohesion: 0.20
Nodes (9): description, identifier, core:default, permissions, $schema, windows, allow-main-commands, main (+1 more)

### Community 17 - "fetch_env.rs"
Cohesion: 0.11
Nodes (41): JoinHandle, a_client_error_fails_at_once_without_retrying(), a_fatal_transport_error_fails_at_once_without_retrying(), a_retryable_transport_error_is_retried(), a_server_error_is_retried_until_it_succeeds(), a_success_returns_the_body_and_the_post_redirect_url(), Ao3Response, backoff_doubles_between_attempts() (+33 more)

## Knowledge Gaps
- **35 isolated node(s):** `$schema`, `identifier`, `description`, `main`, `core:default` (+30 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Db` connect `Db` to `fetch.rs`, `db.rs`, `dispatch.rs`?**
  _High betweenness centrality (0.182) - this node is a cross-community bridge._
- **Why does `ListingStatus` connect `fetch.rs` to `Ffn`, `fetch_session.rs`, `ffn.rs`, `ao3.rs`?**
  _High betweenness centrality (0.081) - this node is a cross-community bridge._
- **Why does `FetchSession` connect `fetch.rs` to `fetch_env.rs`, `fetch_session.rs`?**
  _High betweenness centrality (0.074) - this node is a cross-community bridge._
- **What connects `$schema`, `identifier`, `description` to the rest of the system?**
  _35 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Ffn` be split into smaller, more focused modules?**
  _Cohesion score 0.073224043715847 - nodes in this community are weakly interconnected._
- **Should `fetch.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0958904109589041 - nodes in this community are weakly interconnected._
- **Should `queries.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.09175920514319112 - nodes in this community are weakly interconnected._