//! Page-shape dispatch: classify a fetched URL into a `PageShape` *once*,
//! then let every consumer — `persist_page` (what to parse and write) and
//! `fetch_session.rs` (whether a reported page is the fetch that's waiting) —
//! read that one answer. Before this module the success and error paths
//! walked their own chapter/listing/category-index ladders, kept in the same
//! order only by comments (`fetch.rs`, architecture review 2026-07-29
//! recorded one drift bug between them).
//!
//! Deliberately free of `AppHandle`/`tauri::State`: takes `(html, url,
//! hints)` and a `db::Db`, so every shape is testable from fixtures. The FFN
//! pre-checks (debug dump, Cloudflare-block detection) stay in `fetch.rs`. Parsing stays in `ffn.rs`/`ao3.rs`, reached through
//! `FicSource` (`source.rs`).

use crate::db;
use crate::queries::{self, ChapterContent, ChapterListItem};
use crate::source::{self, ChapterInfo, FicSource, StorySource};

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChapterFetched {
    pub story_id: i64,
    pub chapter_num: i64,
    /// `stories.chapter_count` after this fetch — it can grow on an
    /// in-progress fic, and a caller holding the story row (the Reader)
    /// bounds navigation on it. Read inside the persist transaction.
    pub chapter_count: i64,
}

/// `ensure_chapter`'s result: the chapter is cached and `content` is its body.
/// `chapter_count` and `chapters` are `Some` only when this call fetched
/// (`was_fetched`) — on a cache hit nothing changed, so the caller keeps what
/// it holds. A successful fetch implies the chapter is cached
/// (`persist_page` refuses an empty body), so there is no `is_cached` to check.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsuredChapter {
    pub was_fetched: bool,
    pub chapter_count: Option<i64>,
    pub chapters: Option<Vec<ChapterListItem>>,
    pub content: ChapterContent,
}

/// "A pasted URL means keep it" (`add_story_by_url`): bookmarks the story it
/// just imported. Only the bookmark flag is touched — `isRead`/`dismissed` stay
/// as they were, so re-adding a story you dismissed bookmarks it but leaves it
/// dismissed. Not part of the persist transaction: a failure here leaves the
/// story imported, and re-pasting the URL (idempotent) retries it.
pub fn bookmark_imported_story(db: &db::Db, story_id: i64) -> Result<(), String> {
    db.run(|conn| queries::set_bookmarked(conn, story_id, true))
}

/// `ensure_chapter`'s cache check: the chapter's body if it is already cached.
/// `None` means fetch it (never registered, or a TOC stub).
pub fn cached_chapter(db: &db::Db, story_id: i64, chapter_num: i64) -> Result<Option<EnsuredChapter>, String> {
    let content = db.run(|conn| queries::get_chapter(conn, story_id, chapter_num))?;
    Ok(content.filter(|c| c.is_cached).map(|content| EnsuredChapter {
        was_fetched: false,
        chapter_count: None,
        chapters: None,
        content,
    }))
}

/// `ensure_chapter`'s result after a fetch of `(story_id, chapter_num)`
/// persisted: the fresh chapter list and the story's chapter count alongside
/// the body. Errors if the requested chapter still isn't cached — the fetched
/// page was for a different chapter than the one asked for.
pub fn ensured_after_fetch(
    db: &db::Db,
    story_id: i64,
    chapter_num: i64,
    fetched: &ChapterFetched,
) -> Result<EnsuredChapter, String> {
    let (content, chapters) = db.run(|conn| {
        Ok((queries::get_chapter(conn, story_id, chapter_num)?, queries::list_chapters(conn, story_id)?))
    })?;
    let content = content.filter(|c| c.is_cached).ok_or_else(|| {
        format!("fetched chapter {} of story {}, but chapter {chapter_num} is still not cached", fetched.chapter_num, fetched.story_id)
    })?;
    Ok(EnsuredChapter { was_fetched: true, chapter_count: Some(fetched.chapter_count), chapters: Some(chapters), content })
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BrowseFetched {
    pub fandom: String,
    pub count: i64,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FandomIndexFetched {
    pub category: String,
    pub count: i64,
}

/// What a successful persist produced. `FandomIndex` is FFN-only (SPEC.md
/// §8): `Ao3` never classifies a page as one (`FicSource::category_from_index_url`
/// defaults to `None`), so no caller needs an `unreachable!()` for it.
///
/// Crosses to the frontend only as the raw-URL `fetch_page` return (the
/// debug page); every typed command unwraps it with `into_*`.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FetchOutcome {
    Chapter(ChapterFetched),
    Browse(BrowseFetched),
    FandomIndex(FandomIndexFetched),
}

impl FetchOutcome {
    fn describe(&self) -> &'static str {
        match self {
            FetchOutcome::Chapter(_) => "a chapter page",
            FetchOutcome::Browse(_) => "a browse listing",
            FetchOutcome::FandomIndex(_) => "a category index",
        }
    }

    pub fn into_chapter(self) -> Result<ChapterFetched, String> {
        match self {
            FetchOutcome::Chapter(fetched) => Ok(fetched),
            other => Err(format!("expected a chapter page, got {}", other.describe())),
        }
    }

    pub fn into_browse(self) -> Result<BrowseFetched, String> {
        match self {
            FetchOutcome::Browse(fetched) => Ok(fetched),
            other => Err(format!("expected a browse listing, got {}", other.describe())),
        }
    }

    pub fn into_fandom_index(self) -> Result<FandomIndexFetched, String> {
        match self {
            FetchOutcome::FandomIndex(fetched) => Ok(fetched),
            other => Err(format!("expected a category index, got {}", other.describe())),
        }
    }
}

/// Which of the fetch shapes a URL identifies, with that shape's locator.
/// Pure URL parsing — no DB, no HTML.
pub enum PageShape<S: FicSource> {
    FandomIndex(String),
    Chapter(S::ChapterLocator),
    Listing(S::ListingLocator),
    Unknown,
}

/// The one classification order, shared by `persist_page` and
/// `fetch_session::TargetKey`: category index, then chapter, then listing.
///
/// A matched chapter locator is definitive — listing/category parsing don't
/// know to reject `/s/{id}/{chapter}/...` shapes, so checking chapter before
/// listing is what keeps a chapter URL from misclassifying as a browse URL.
/// `Err` means "this *is* the chapter shape, but malformed" (see
/// `FicSource::chapter_locator_from_url`) and is propagated, not swallowed.
pub fn classify_page<S: FicSource>(url: &str) -> Result<PageShape<S>, String> {
    if let Some(category) = S::category_from_index_url(url) {
        return Ok(PageShape::FandomIndex(category));
    }
    if let Some(locator) = S::chapter_locator_from_url(url)? {
        return Ok(PageShape::Chapter(locator));
    }
    if let Some(locator) = S::listing_locator_from_url(url) {
        return Ok(PageShape::Listing(locator));
    }
    Ok(PageShape::Unknown)
}

/// Validates a URL a user typed to add one story, and says which source
/// fetches it. Accepts only an http(s) URL on that source's own host whose
/// path classifies (`classify_page`) as a `Chapter` shape — a listing or
/// category URL would otherwise import a whole page of stories as a side
/// effect. The locators themselves only look at the path (`ffn.rs`/`ao3.rs`),
/// so the host check lives here, not in them.
pub fn story_url_source(url: &str) -> Result<StorySource, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|_| "not a valid URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("not a valid URL (expected http or https)".to_string());
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let url = parsed.as_str();
    match host.as_str() {
        "fanfiction.net" | "www.fanfiction.net" => match classify_page::<source::Ffn>(url) {
            Ok(PageShape::Chapter(_)) => Ok(StorySource::Ffn),
            _ => Err("not a FanFiction.net story URL — expected https://www.fanfiction.net/s/{id}/{chapter}/…".to_string()),
        },
        "archiveofourown.org" | "www.archiveofourown.org" => match classify_page::<source::Ao3>(url) {
            Ok(PageShape::Chapter(_)) => Ok(StorySource::Ao3),
            _ => Err("not an AO3 work URL — expected https://archiveofourown.org/works/{id}".to_string()),
        },
        _ => Err("only fanfiction.net and archiveofourown.org story URLs are supported".to_string()),
    }
}

/// Resolves a stored story's chapter to the source it belongs to and the URL
/// that fetches it — the one place a chapter fetch learns its URL, so callers
/// (`fetch::fetch_chapter`) need only `(story_id, chapter_num)`. Errors when
/// the story isn't in the DB, before any fetch starts.
pub fn chapter_fetch_url(db: &db::Db, story_id: i64, chapter_num: i64) -> Result<(StorySource, String), String> {
    let (source, source_story_id, source_chapter_id) = db
        .run(|conn| db::chapter_fetch_target(conn, story_id, chapter_num))?
        .ok_or_else(|| format!("no story with id {story_id}"))?;
    let url = match source {
        StorySource::Ffn => source::Ffn::chapter_url(source_story_id, chapter_num, source_chapter_id),
        StorySource::Ao3 => source::Ao3::chapter_url(source_story_id, chapter_num, source_chapter_id),
    };
    Ok((source, url))
}

/// Parse + persist a fetched page for any `FicSource`. Called by the fetch
/// command that is waiting on the page (`fetch.rs`), which returns the
/// `FetchOutcome` (or the error) straight to its caller.
///
/// Each arm parses first and only then takes the DB lock (`Db::run_mut`), so
/// the one `Mutex<Connection>` is held for the transaction alone — a UI query
/// never waits behind an HTML parse.
pub fn persist_page<S: FicSource>(
    db: &db::Db,
    html: &str,
    page_url: &str,
    fandom_override: Option<&str>,
    requested_listing_locator: Option<S::ListingLocator>,
) -> Result<FetchOutcome, String> {
    match classify_page::<S>(page_url)? {
        PageShape::FandomIndex(category) => {
            let entries = S::parse_category_index(html);
            if entries.is_empty() {
                return Err("no fandoms found on this page — not a category index, or the layout changed".to_string());
            }
            db.run_mut(|conn| db::replace_fandom_index(conn, &category, &entries))?;
            println!("{} indexed {} fandoms (1000+ fics) for category {category}", S::LOG_PREFIX, entries.len());
            Ok(FetchOutcome::FandomIndex(FandomIndexFetched { category, count: entries.len() as i64 }))
        }
        PageShape::Chapter(locator) => {
            let (meta, chapters) = S::parse_work(html, &locator)?;
            let content = S::parse_chapter_content(html)?;
            // A chapter with no body would be stored as a TOC stub's `''` —
            // indistinguishable from "never fetched", so it would retry forever
            // while every fetch reported success. Refuse it here, once, for both
            // sources (CONTEXT.md "Cached chapter").
            if content.trim().is_empty() {
                return Err("chapter page has no content — not a chapter, or the layout changed".to_string());
            }
            let chapter_num = S::resolve_chapter_num(&chapters, &locator)?;
            let matched = chapters.iter().find(|c| c.chapter_num() == chapter_num);
            let chapter_title = matched.map(|c| c.title()).unwrap_or_else(|| S::metadata_title(&meta));
            let source_chapter_id = matched.and_then(|c| c.source_chapter_id());

            println!(
                "{} {} — chapter {chapter_num} ({} bytes), {} chapters in TOC",
                S::LOG_PREFIX,
                S::metadata_title(&meta),
                content.len(),
                chapters.len()
            );

            // One transaction: a failure mid-TOC leaves nothing behind, and a
            // 300-chapter import is one commit instead of 300+.
            //
            // Architecture review candidate 3 (2026-10-07), rejected: wrapping
            // this sequence in one `save_story_page` write module. There is no
            // ordering rule to hide — `upsert_chapter_content` is insert-or-update,
            // so stubs-before-content is not required; only `story_id` feeds the
            // later calls — and atomicity is already covered by
            // `chapter_page_failing_mid_toc_leaves_no_story_behind`. Reopen if a
            // third caller needs the same write.
            let (story_id, chapter_count) = db.run_mut(|conn| {
                let tx = conn.transaction()?;
                let story_id = S::persist_story(&tx, &meta)?;
                db::ensure_read_state(&tx, story_id)?;
                // Register the *full* TOC every time — any story page lists every
                // chapter, so this keeps titles browsable even before their own
                // pages have been fetched. Only the chapter we actually
                // navigated to gets content.
                for c in &chapters {
                    db::upsert_chapter_stub(&tx, story_id, c.chapter_num(), c.title(), c.source_chapter_id())?;
                }
                db::upsert_chapter_content(&tx, story_id, chapter_num, chapter_title, &content, source_chapter_id)?;
                let chapter_count: i64 =
                    tx.query_row("SELECT chapter_count FROM stories WHERE id = ?1", [story_id], |row| row.get(0))?;
                tx.commit()?;
                Ok((story_id, chapter_count))
            })?;

            Ok(FetchOutcome::Chapter(ChapterFetched { story_id, chapter_num, chapter_count }))
        }
        PageShape::Listing(locator) => {
            let mut summaries = S::parse_listing(html, &locator)?;
            if summaries.is_empty() {
                return Err("no stories found on this page — not a listing, or the layout changed".to_string());
            }
            if let Some(label) = fandom_override {
                for meta in &mut summaries {
                    S::apply_fandom_override(meta, label);
                }
            }
            let fandom = S::metadata_fandom(&summaries[0]).to_string();
            let prefix = S::LOG_PREFIX;
            // Recorded under the locator the *request* carried, never the one
            // decoded from the fetched URL (`locator`, which only feeds
            // `parse_listing`): FFN's URL encoding is lossy for status, and a
            // redirected AO3 URL needn't carry the tag that was asked for. No
            // requested locator (a raw-URL debug fetch) means no search
            // session worth bookkeeping. See `FetchedPage` (`fetch_session.rs`).
            let recorded_page = db.run_mut(|conn| {
                let tx = conn.transaction()?;
                for meta in &summaries {
                    let story_id = S::persist_story(&tx, meta)?;
                    db::ensure_read_state(&tx, story_id)?;
                }
                let page = match requested_listing_locator {
                    Some(record_locator) => {
                        let page = S::listing_page(&record_locator);
                        S::record_listing_page(&tx, &record_locator, page)?;
                        Some(page)
                    }
                    None => None,
                };
                tx.commit()?;
                Ok(page)
            })?;
            match recorded_page {
                Some(page) => println!("{prefix} imported {} stories into fandom {fandom:?}, page {page}", summaries.len()),
                None => println!(
                    "{prefix} imported {} stories into fandom {fandom:?} (no search session to bookkeep)",
                    summaries.len()
                ),
            }
            Ok(FetchOutcome::Browse(BrowseFetched { fandom, count: summaries.len() as i64 }))
        }
        PageShape::Unknown => Err("could not determine what kind of page this is (chapter, or listing)".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ao3, ffn, source};

    const FFN_STORY_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_story_page.html"));
    const FFN_BROWSE_LISTING: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_browse_listing.html"));
    const FFN_CATEGORY_INDEX: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_category_index.html"));
    const AO3_WORK_PAGE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ao3_work_page.html"));
    const AO3_LISTING_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ao3_listing_page.html"));

    fn test_db() -> db::Db {
        db::Db(std::sync::Mutex::new(db::tests::memory_conn()))
    }

    #[test]
    fn ffn_chapter_url_dispatches_to_chapter_branch_and_resolves_chapter_num() {
        let dbh = test_db();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

        let outcome = persist_page::<source::Ffn>(&dbh, FFN_STORY_PAGE, url, None, None).unwrap();

        match outcome {
            FetchOutcome::Chapter(fetched) => assert_eq!(fetched.chapter_num, 34),
            _ => panic!("expected a Chapter outcome for a chapter URL"),
        }
    }


    const FFN_CHAPTER_URL: &str = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

    fn persist_fixture_chapter(dbh: &db::Db) -> ChapterFetched {
        persist_page::<source::Ffn>(dbh, FFN_STORY_PAGE, FFN_CHAPTER_URL, None, None).unwrap().into_chapter().unwrap()
    }

    #[test]
    fn chapter_fetched_carries_the_stories_chapter_count() {
        let dbh = test_db();

        let fetched = persist_fixture_chapter(&dbh);

        let stored: i64 = dbh
            .run(|conn| conn.query_row("SELECT chapter_count FROM stories WHERE id = ?1", [fetched.story_id], |r| r.get(0)))
            .unwrap();
        assert_eq!(fetched.chapter_count, stored);
        assert!(fetched.chapter_count >= 34, "the fixture is a 144-chapter story viewed at chapter 34");
    }

    #[test]
    fn chapter_page_with_an_empty_body_is_rejected_and_stores_nothing() {
        let dbh = test_db();
        // Blank the #storytext element's contents in the raw page (the body has
        // no nested <div>, so the first </div> after the open tag closes it).
        let open = FFN_STORY_PAGE.find("id=\"storytext\">").expect("fixture has #storytext");
        let body_start = open + FFN_STORY_PAGE[open..].find('>').unwrap() + 1;
        let body_end = body_start + FFN_STORY_PAGE[body_start..].find("</div>").unwrap();
        let emptied = format!("{}  {}", &FFN_STORY_PAGE[..body_start], &FFN_STORY_PAGE[body_end..]);
        assert_eq!(ffn::parse_chapter_content(&emptied).unwrap().trim(), "", "fixture edit must empty #storytext");

        let err = persist_page::<source::Ffn>(&dbh, &emptied, FFN_CHAPTER_URL, None, None).unwrap_err();

        assert!(err.contains("no content"), "got: {err}");
        let stories: i64 = dbh.run(|conn| conn.query_row("SELECT COUNT(*) FROM stories", [], |r| r.get(0))).unwrap();
        assert_eq!(stories, 0, "a rejected page must not leave a story or TOC stubs behind");
    }

    #[test]
    fn bookmark_imported_story_bookmarks_only_that_flag() {
        let dbh = test_db();
        let fetched = persist_fixture_chapter(&dbh);
        dbh.run(|conn| queries::set_dismissed(conn, fetched.story_id, true)).unwrap();

        bookmark_imported_story(&dbh, fetched.story_id).unwrap();
        bookmark_imported_story(&dbh, fetched.story_id).unwrap(); // re-adding is idempotent

        let row = dbh.run(|conn| queries::get_story(conn, fetched.story_id)).unwrap().unwrap();
        assert!(row.bookmarked);
        assert!(row.dismissed && !row.is_read, "flags are independent");
    }

    #[test]
    fn cached_chapter_misses_a_toc_stub_and_hits_a_fetched_chapter() {
        let dbh = test_db();
        let fetched = persist_fixture_chapter(&dbh);

        // Chapter 1 is only a TOC stub; chapter 34 has its body.
        assert!(cached_chapter(&dbh, fetched.story_id, 1).unwrap().is_none());
        assert!(cached_chapter(&dbh, fetched.story_id, 9999).unwrap().is_none(), "unregistered chapter");
        let hit = cached_chapter(&dbh, fetched.story_id, 34).unwrap().expect("chapter 34 is cached");
        assert!(!hit.was_fetched);
        assert!(hit.chapter_count.is_none() && hit.chapters.is_none(), "nothing changed on a hit");
        assert!(hit.content.is_cached && hit.content.content.len() > 500);
    }

    #[test]
    fn ensured_after_fetch_returns_body_chapter_list_and_count() {
        let dbh = test_db();
        let fetched = persist_fixture_chapter(&dbh);

        let ensured = ensured_after_fetch(&dbh, fetched.story_id, 34, &fetched).unwrap();

        assert!(ensured.was_fetched);
        assert_eq!(ensured.chapter_count, Some(fetched.chapter_count));
        let chapters = ensured.chapters.expect("a fetch returns the refreshed list");
        assert_eq!(chapters.len() as i64, fetched.chapter_count);
        assert_eq!(chapters.iter().filter(|c| c.is_cached).map(|c| c.chapter_num).collect::<Vec<_>>(), vec![34]);
        assert!(ensured.content.is_cached);
    }

    #[test]
    fn ensured_after_fetch_errors_when_the_requested_chapter_is_still_not_cached() {
        let dbh = test_db();
        let fetched = persist_fixture_chapter(&dbh);

        // Asked for chapter 1 but the page that came back was chapter 34.
        let err = ensured_after_fetch(&dbh, fetched.story_id, 1, &fetched).unwrap_err();

        assert!(err.contains("chapter 1 is still not cached"), "got: {err}");
    }

    #[test]
    fn ffn_listing_url_dispatches_to_browse_branch() {
        let dbh = test_db();
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::Any, 1);

        let outcome = persist_page::<source::Ffn>(&dbh, FFN_BROWSE_LISTING, &url, None, None).unwrap();

        match outcome {
            FetchOutcome::Browse(fetched) => assert_eq!(fetched.count, 25),
            _ => panic!("expected a Browse outcome for a listing URL"),
        }
    }


    #[test]
    fn ffn_in_progress_listing_records_the_real_requested_status_not_the_lossy_decoded_one() {
        let dbh = test_db();
        // build_browse_url can't distinguish "in_progress" from "any" in the
        // URL itself (see FfnListingLocator's doc comment) — decoding this
        // URL alone would always recover "any", which is the bug this test
        // guards against.
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::InProgress, 1);
        let requested = source::FfnListingLocator {
            category: "anime".to_string(),
            fandom: "Fullmetal Alchemist".to_string(),
            min_word_count: 0,
            status: crate::source::ListingStatus::InProgress,
            page: 1,
        };

        persist_page::<source::Ffn>(&dbh, FFN_BROWSE_LISTING, &url, None, Some(requested)).unwrap();

        let conn = dbh.0.lock().unwrap();
        let recorded_status: String = conn
            .query_row("SELECT status FROM browse_state WHERE fandom = ?1", ["Fullmetal Alchemist"], |row| row.get(0))
            .unwrap();
        assert_eq!(recorded_status, "in_progress", "must record under the real requested status, not the URL-decoded one");
    }


    #[test]
    fn ao3_listing_records_under_the_requested_tag_even_when_the_fetched_url_names_another() {
        let dbh = test_db();
        // AO3 canonicalised the tag: the page that came back says "Naruto (Anime & Manga)".
        let fetched_url = ao3::build_tag_browse_url("Naruto (Anime & Manga)", "bookmarks_count", 0, crate::source::ListingStatus::Any, 1);
        let requested = source::Ao3ListingLocator {
            tag: "Naruto".to_string(),
            min_word_count: 0,
            status: crate::source::ListingStatus::Any,
            page: 1,
        };

        persist_page::<source::Ao3>(&dbh, AO3_LISTING_PAGE, &fetched_url, None, Some(requested)).unwrap();

        assert_eq!(dbh.run(|c| db::next_ao3_browse_page(c, "Naruto", 0, "any")).unwrap(), 2);
        assert_eq!(dbh.run(|c| db::next_ao3_browse_page(c, "Naruto (Anime & Manga)", 0, "any")).unwrap(), 1);
    }

    #[test]
    fn ao3_listing_fetch_with_no_requested_locator_does_not_record_browse_state() {
        let dbh = test_db();
        let url = ao3::build_tag_browse_url("Naruto", "bookmarks_count", 0, crate::source::ListingStatus::Any, 1);

        persist_page::<source::Ao3>(&dbh, AO3_LISTING_PAGE, &url, None, None).unwrap();

        let count: i64 = dbh.run(|c| c.query_row("SELECT COUNT(*) FROM ao3_browse_state", [], |r| r.get(0))).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn ffn_listing_fetch_with_no_requested_locator_does_not_record_browse_state() {
        // The debug harness's raw-URL fetch has no real search session behind
        // it — see the comment in `persist_page`'s listing arm.
        let dbh = test_db();
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::Any, 1);

        persist_page::<source::Ffn>(&dbh, FFN_BROWSE_LISTING, &url, None, None).unwrap();

        let conn = dbh.0.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM browse_state", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "no locator stashed means no real search session to bookkeep");
    }


    #[test]
    fn empty_listing_page_is_an_error_not_a_zero_count_browse_outcome() {
        let dbh = test_db();
        let url = ffn::build_browse_url("anime", "Nonexistent Fandom", 0, crate::source::ListingStatus::Any, 1);

        // FFN_STORY_PAGE is a chapter page, not a listing — parse_browse_page
        // against it should find zero story rows.
        let result = persist_page::<source::Ffn>(&dbh, FFN_STORY_PAGE, &url, None, None);

        assert!(result.is_err(), "expected an error for a listing page with zero story rows");
    }


    fn count(dbh: &db::Db, table: &str) -> i64 {
        let conn = dbh.0.lock().unwrap();
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn chapter_page_failing_mid_toc_leaves_no_story_behind() {
        let dbh = test_db();
        dbh.0
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_chapter BEFORE INSERT ON chapters
                 BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            )
            .unwrap();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

        let result = persist_page::<source::Ffn>(&dbh, FFN_STORY_PAGE, url, None, None);

        assert!(result.is_err());
        assert_eq!(count(&dbh, "stories"), 0, "story row must roll back with its chapters");
        assert_eq!(count(&dbh, "read_state"), 0);
    }

    #[test]
    fn listing_page_failing_mid_rows_leaves_no_rows_or_page_record() {
        let dbh = test_db();
        dbh.0
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_third BEFORE INSERT ON read_state
                 WHEN (SELECT COUNT(*) FROM read_state) >= 2
                 BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            )
            .unwrap();
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::Any, 1);

        let result = persist_page::<source::Ffn>(&dbh, FFN_BROWSE_LISTING, &url, None, None);

        assert!(result.is_err());
        assert_eq!(count(&dbh, "stories"), 0);
        assert_eq!(count(&dbh, "read_state"), 0);
    }

    #[test]
    fn ao3_fandom_override_applies_on_listing_rows() {
        let dbh = test_db();
        let url = ao3::build_tag_browse_url("Harry Potter", "bookmarks_count", 0, crate::source::ListingStatus::Any, 1);

        persist_page::<source::Ao3>(&dbh, AO3_LISTING_PAGE, &url, Some("Overridden Fandom"), None).unwrap();

        let conn = dbh.0.lock().unwrap();
        let fandom: String = conn.query_row("SELECT fandom FROM stories LIMIT 1", [], |row| row.get(0)).unwrap();
        assert_eq!(fandom, "Overridden Fandom");
    }


    #[test]
    fn ao3_fandom_override_is_ignored_on_the_chapter_path() {
        let dbh = test_db();
        let url = "https://archiveofourown.org/works/85954646/chapters/227238471";

        let outcome =
            persist_page::<source::Ao3>(&dbh, AO3_WORK_PAGE, url, Some("Should Not Apply Here"), None).unwrap();

        match outcome {
            FetchOutcome::Chapter(fetched) => {
                let conn = dbh.0.lock().unwrap();
                let fandom: String =
                    conn.query_row("SELECT fandom FROM stories WHERE id = ?1", [fetched.story_id], |row| row.get(0)).unwrap();
                assert_eq!(fandom, "Original Work, Naruto (Anime & Manga)");
            }
            _ => panic!("expected a Chapter outcome for a work/chapter URL"),
        }
    }


    #[test]
    fn into_chapter_unwraps_a_chapter_outcome_and_rejects_the_other_shapes() {
        let chapter = FetchOutcome::Chapter(ChapterFetched { story_id: 7, chapter_num: 2, chapter_count: 9 });
        assert_eq!(chapter.into_chapter().unwrap().story_id, 7);

        let browse = FetchOutcome::Browse(BrowseFetched { fandom: "x".to_string(), count: 1 });
        assert_eq!(browse.into_chapter().unwrap_err(), "expected a chapter page, got a browse listing");
    }

    #[test]
    fn into_browse_and_into_fandom_index_reject_a_mismatched_outcome() {
        let index = FetchOutcome::FandomIndex(FandomIndexFetched { category: "anime".to_string(), count: 3 });
        assert_eq!(index.into_browse().unwrap_err(), "expected a browse listing, got a category index");

        let chapter = FetchOutcome::Chapter(ChapterFetched { story_id: 1, chapter_num: 1, chapter_count: 1 });
        assert_eq!(chapter.into_fandom_index().unwrap_err(), "expected a category index, got a chapter page");
    }

    #[test]
    fn fetch_outcome_serializes_with_a_kind_tag_the_debug_page_can_read() {
        let json = serde_json::to_value(FetchOutcome::Chapter(ChapterFetched { story_id: 7, chapter_num: 2, chapter_count: 9 })).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "chapter", "storyId": 7, "chapterNum": 2, "chapterCount": 9 }));
    }

    #[test]
    fn chapter_fetch_url_builds_the_ffn_chapter_url_from_the_stored_story() {
        let dbh = test_db();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";
        let story_id = match persist_page::<source::Ffn>(&dbh, FFN_STORY_PAGE, url, None, None).unwrap() {
            FetchOutcome::Chapter(f) => f.story_id,
            _ => panic!("expected a Chapter outcome"),
        };

        let (src, fetch_url) = chapter_fetch_url(&dbh, story_id, 7).unwrap();

        assert_eq!(src, StorySource::Ffn);
        assert_eq!(fetch_url, "https://www.fanfiction.net/s/13493099/7/");
    }

    #[test]
    fn chapter_fetch_url_uses_the_recorded_ao3_chapter_id_and_falls_back_to_the_bare_work() {
        let dbh = test_db();
        let url = "https://archiveofourown.org/works/85954646/chapters/227238471";
        let story_id = match persist_page::<source::Ao3>(&dbh, AO3_WORK_PAGE, url, None, None).unwrap() {
            FetchOutcome::Chapter(f) => f.story_id,
            _ => panic!("expected a Chapter outcome"),
        };
        let known = dbh
            .run(|c| c.query_row(
                "SELECT chapter_num FROM chapters WHERE story_id = ?1 AND source_chapter_id = 227238471",
                [story_id],
                |r| r.get::<_, i64>(0),
            ))
            .unwrap();

        let (src, with_id) = chapter_fetch_url(&dbh, story_id, known).unwrap();
        let (_, without_id) = chapter_fetch_url(&dbh, story_id, 9999).unwrap();

        assert_eq!(src, StorySource::Ao3);
        assert_eq!(with_id, "https://archiveofourown.org/works/85954646/chapters/227238471");
        assert_eq!(without_id, "https://archiveofourown.org/works/85954646");
    }

    #[test]
    fn chapter_fetch_url_rejects_an_unknown_story_before_any_fetch() {
        let dbh = test_db();
        assert!(chapter_fetch_url(&dbh, 424242, 1).is_err());
    }

    #[test]
    fn classify_page_checks_category_index_before_chapter_and_listing() {
        // One order for persist and error targeting: the FFN category index
        // is checked first, so no later shape's check can swallow it.
        assert!(matches!(
            classify_page::<source::Ffn>("https://www.fanfiction.net/anime/"),
            Ok(PageShape::FandomIndex(c)) if c == "anime"
        ));
    }

    #[test]
    fn classify_page_distinguishes_chapter_listing_and_unknown() {
        let chapter = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";
        assert!(matches!(classify_page::<source::Ffn>(chapter), Ok(PageShape::Chapter(_))));

        let listing = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::Any, 1);
        assert!(matches!(classify_page::<source::Ffn>(&listing), Ok(PageShape::Listing(_))));

        assert!(matches!(classify_page::<source::Ffn>("https://example.com/"), Ok(PageShape::Unknown)));
    }

    #[test]
    fn ao3_never_classifies_a_category_index() {
        assert!(!matches!(
            classify_page::<source::Ao3>("https://archiveofourown.org/anime/"),
            Ok(PageShape::FandomIndex(_))
        ));
    }

    #[test]
    fn ffn_category_index_page_persists_a_fandom_index_outcome() {
        // Through the same `persist_page` interface as every other shape —
        // previously this branch lived only in fetch.rs's FFN-specific wrapper.
        let dbh = test_db();
        let outcome = persist_page::<source::Ffn>(&dbh, FFN_CATEGORY_INDEX, "https://www.fanfiction.net/anime/", None, None).unwrap();

        match outcome {
            FetchOutcome::FandomIndex(f) => {
                assert_eq!(f.category, "anime");
                assert!(f.count > 0);
            }
            _ => panic!("expected a FandomIndex outcome for a category index URL"),
        }
        let conn = dbh.0.lock().unwrap();
        let rows = db::list_fandom_index(&conn, Some("anime")).unwrap();
        assert!(!rows.is_empty(), "index rows must be persisted, not just counted");
    }

    #[test]
    fn category_index_page_with_no_fandoms_is_an_error() {
        let dbh = test_db();
        let result = persist_page::<source::Ffn>(&dbh, "<html></html>", "https://www.fanfiction.net/anime/", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn story_url_source_accepts_ffn_and_ao3_story_urls() {
        for url in [
            "https://www.fanfiction.net/s/14583034/1/Harry-Potter-The-Game",
            "https://fanfiction.net/s/14583034/3/",
            "  https://www.fanfiction.net/s/14583034/1/Slug  ",
        ] {
            assert_eq!(story_url_source(url), Ok(StorySource::Ffn), "{url}");
        }
        for url in [
            "https://archiveofourown.org/works/85954646",
            "https://archiveofourown.org/works/85954646/chapters/227238471",
            "https://www.archiveofourown.org/works/85954646",
        ] {
            assert_eq!(story_url_source(url), Ok(StorySource::Ao3), "{url}");
        }
    }

    #[test]
    fn story_url_source_rejects_non_story_shapes_even_on_a_known_host() {
        // A listing/category URL would import a page of stories as a side effect.
        let listing = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::Any, 1);
        for url in [
            listing.as_str(),
            "https://www.fanfiction.net/anime/",
            "https://www.fanfiction.net/u/12345/someone",
            "https://archiveofourown.org/tags/Naruto/works",
        ] {
            assert!(story_url_source(url).is_err(), "{url}");
        }
    }

    #[test]
    fn story_url_source_rejects_other_hosts_schemes_and_garbage() {
        // The locators only read the path, so the host check is what stops these.
        for url in [
            "https://example.com/s/14583034/1/x",
            "https://example.com/works/85954646",
            "https://m.fanfiction.net/s/14583034/1/x",
            "ftp://www.fanfiction.net/s/14583034/1/x",
            "file:///s/14583034/1/x",
            "not a url",
            "",
        ] {
            assert!(story_url_source(url).is_err(), "{url}");
        }
    }

    #[test]
    fn story_url_source_rejects_a_malformed_chapter_segment() {
        assert!(story_url_source("https://www.fanfiction.net/s/abc/1/x").is_err());
    }
}
