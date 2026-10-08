//! Fetch orchestration: driving `FicSource` (`source.rs`) dispatch against
//! the two concrete fetch mechanics (FFN's webview round-trip, AO3's direct
//! HTTP call), both reached through `fetch_env.rs`'s `FetchEnv`. Every fetch
//! command returns its parsed-and-persisted result
//! directly — `ChapterFetched`/`BrowseFetched`/`FandomIndexFetched` — or an
//! error string; there are no result events to correlate. The FFN webview's
//! in-flight bookkeeping (supersede, timeout, which report belongs to which
//! fetch) lives in `fetch_session.rs`; persisting happens here, in the
//! command that is waiting, the same shape as the AO3 path.
//! `source.rs`'s own doc comment (architecture review candidate 1,
//! 2026-07-19) already establishes that the trait lives there and the
//! dispatch driver lives outside it — this module is that driver's home,
//! split out of `lib.rs` (architecture review candidate, 2026-08-05) so
//! command registration in `lib.rs` stays a thin list instead of
//! interleaving with ~750 lines of fetch machinery.
//!
//! Every command is a thin Tauri adapter over a `*_via` function that takes the
//! `FetchEnv`, the `Db` and the `FetchSession` explicitly (architecture review
//! candidate, 2026-10-08), so each fetch sequence runs from tests with
//! `fetch_env::testing::FakeEnv`.

use crate::dispatch::{self, BrowseFetched, ChapterFetched, EnsuredChapter, FandomIndexFetched, FetchOutcome};
use crate::fetch_env::{get_with_retry, FetchEnv, TauriEnv};
use crate::fetch_session::{FetchSession, Priority};
use crate::source::{self, ListingStatus, StorySource};
use crate::{ao3, db, ffn};

/// Scoped to the `<title>` (never full page text) — story prose is free-form
/// English and can innocently contain phrases like "just a moment"; a
/// Cloudflare block page's *title* is one of a small fixed set that a real
/// FFN chapter title (always "<story> Chapter N: ... | FanFiction") can't
/// plausibly collide with. `cf-browser-verification` is a real CSS class
/// Cloudflare's interstitial injects, safe to check against the full page.
///
/// Architecture review candidate 2 (2026-07-21): pulled free of `AppHandle`/
/// `tauri::State` (the same move already made for `try_persist_fetch`) so
/// this heuristic — previously untestable — has direct unit test coverage,
/// including its known false-positive risk (a real fic titled with one of
/// the trigger phrases).
fn looks_blocked(html: &str, title: &str) -> bool {
    let title_lower = title.to_lowercase();
    title_lower.contains("just a moment")
        || title_lower.contains("attention required")
        || title_lower.contains("checking your browser")
        || html.contains("cf-browser-verification")
}

/// FFN-specific pre-check (Cloudflare-block detection), then the shared
/// `dispatch::persist_page::<source::Ffn>` — which also owns FFN's
/// category-index shape, so there is no FFN-only branch left here. Free of
/// `AppHandle` (the debug dump lives in `fetch_ffn_via`) so it runs from fixtures.
fn persist_ffn_page(
    db: &db::Db,
    html: &str,
    page_url: &str,
    title: &str,
    listing_locator: Option<source::FfnListingLocator>,
) -> Result<FetchOutcome, String> {
    if looks_blocked(html, title) {
        return Err("Cloudflare challenge page, not the story — try again".to_string());
    }

    if html.contains(NO_SUCH_CHAPTER_MARKER) {
        return Err(format!(
            "FFN returned its {NO_SUCH_CHAPTER_MARKER:?} page for this chapter — often temporary (try again), not necessarily removed"
        ));
    }

    dispatch::persist_page::<source::Ffn>(db, html, page_url, None, listing_locator)
}

/// FFN answers a chapter URL it doesn't have with a bare message page (no
/// `#profile_top`), which otherwise surfaces as a confusing parse error.
/// Not permanent: story 470 ch 20 got this page three times in a row, then
/// loaded normally in a browser and in the app later — so it is retried like
/// any other failure, and the error must not claim the chapter is gone.
const NO_SUCH_CHAPTER_MARKER: &str = "Story does not have any chapters";

/// How many times `fetch_ffn_via` waits again after a Cloudflare page is reported.
const BLOCKED_REARMS: u32 = 2;

/// One FFN fetch end to end: navigate the webview, wait for the session to
/// deliver the rendered page (or reject: interrupted, timed out), persist it.
/// The whole sequence — priority refusal, navigation failure, Cloudflare
/// re-arm, timeout, persist — runs from tests against a `FakeEnv`.
async fn fetch_ffn_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    url: String,
    listing_locator: Option<source::FfnListingLocator>,
    priority: Priority,
) -> Result<FetchOutcome, String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    let timeout = env.ffn_wait();

    let fetch = {
        let _window = session.lock_window().await;
        // A refused background fetch returns before touching the webview.
        let fetch = match priority {
            Priority::Foreground => session.start(&url, listing_locator),
            Priority::Background => session.start_background(&url, listing_locator)?,
        };
        if let Err(e) = env.navigate(parsed) {
            session.abort(&fetch, e.clone());
            return Err(e);
        }
        fetch
    };

    let mut page = session.wait(fetch, timeout).await.inspect_err(|e| eprintln!("[fetch] failed: {e}"))?;
    // A Cloudflare interstitial reports on its own DOMContentLoaded, then
    // redirects itself to the real page, whose report would be dropped as
    // matching no pending fetch. Re-arm the session (no navigate — the webview
    // is already on its way) and wait for that second report instead of
    // failing the chapter outright.
    for _ in 0..BLOCKED_REARMS {
        if !looks_blocked(&page.html, &page.title) {
            break;
        }
        let Some(next) = session.start_if_idle(&url, page.listing_locator.clone(), priority) else { break };
        println!("[fetch] challenge page for {url}, waiting for the real page");
        page = session.wait(next, timeout).await.inspect_err(|e| eprintln!("[fetch] failed: {e}"))?;
    }
    // Dumped before the blocked check so a false positive there can still be
    // inspected, not just a genuine block.
    env.save_debug_page("debug-last-fetch.html", &page.html);
    persist_ffn_page(db, &page.html, &page.href, &page.title, page.listing_locator)
        .inspect_err(|e| eprintln!("[fetch] failed: {e}"))
}

/// The Cloudflare/fetch strategy locked in SPEC.md §3: open a real webview
/// pointed at fanfiction.net (not a bare HTTP client) and let
/// `report_fetch_result` pick up the rendered HTML once the page finishes
/// loading. Used both by the debug tools (arbitrary URL) and by the Reader
/// (fetching a chapter that isn't cached yet) — same mechanism either way.
///
/// Reuses one `fetch-page` window across every fetch for the life of the
/// app rather than destroying/rebuilding it per call: on macOS, creating a
/// *new* NSWindow activates the app regardless of `.focused(false)` (that
/// flag only controls focus once the app is already frontmost) — a bulk
/// download hitting this once per chapter was yanking focus back on every
/// single one, even while `visible(false)`. `navigate()` on an existing
/// webview doesn't create a window, so it doesn't activate anything; the
/// window is only ever *built* once (first fetch of the app session).
///
/// Resolves once the page has been fetched *and* persisted, with what that
/// produced. A fetch superseded by a newer one rejects with
/// `"interrupted by a newer fetch"`; one the webview never reports back from
/// rejects after `FETCH_TIMEOUT`. A debug command (raw URL, no locator): the
/// exact `FfnListingLocator` a listing fetch is recorded under only ever comes
/// from `search_ffn`'s in-process call — see `FetchSession`.
#[tauri::command]
pub async fn fetch_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    url: String,
) -> Result<FetchOutcome, String> {
    fetch_ffn_via(&TauriEnv(&app), &db, &session, url, None, Priority::Foreground).await
}

/// Called by the injected init script (`fetch_page_init.js`) once per
/// navigation with the rendered page. Only hands it to the fetch session — a
/// report that matches no pending fetch (superseded, timed out) is dropped.
#[tauri::command]
pub async fn report_fetch_result(
    session: tauri::State<'_, FetchSession>,
    html: String,
    page_url: String,
    title: String,
) -> Result<(), ()> {
    if !session.deliver(&page_url, html, title) {
        println!("[fetch] dropped report for {page_url}: no matching pending fetch");
    }
    Ok(())
}

/// AO3 counterpart to `fetch_ffn_via` — no webview/session machinery (see
/// `fetch_env.rs`'s AO3 notes): a direct HTTP call with retry, then persist.
/// `fandom_override`, when given, force-applies that label to every row of a
/// tag-listing fetch (ignored for a work/chapter fetch) — see
/// `source::Ao3::apply_fandom_override`. `requested`, for a listing, is the
/// locator `search_ao3_via` built the URL from: the fetch is recorded under it,
/// not under whatever the (possibly redirected) final URL says.
async fn fetch_ao3_via(
    env: &impl FetchEnv,
    db: &db::Db,
    url: &str,
    fandom_override: Option<&str>,
    requested: Option<source::Ao3ListingLocator>,
) -> Result<FetchOutcome, String> {
    let (html, final_url) = get_with_retry(env, url).await.inspect_err(|e| eprintln!("[ao3-fetch] failed: {e}"))?;
    // Dumped unconditionally, same posture as the FFN path's own
    // debug-last-fetch.html — a failed parse should still be inspectable.
    env.save_debug_page("debug-last-ao3-fetch.html", &html);
    dispatch::persist_page::<source::Ao3>(db, &html, &final_url, fandom_override, requested)
        .inspect_err(|e| eprintln!("[ao3-fetch] failed: {e}"))
}

/// Fetches `url` through whichever mechanism its source needs.
async fn fetch_by_source_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    source: StorySource,
    url: String,
    priority: Priority,
) -> Result<FetchOutcome, String> {
    match source {
        StorySource::Ffn => fetch_ffn_via(env, db, session, url, None, priority).await,
        StorySource::Ao3 => fetch_ao3_via(env, db, &url, None, None).await,
    }
}

/// Adds one story by pasted URL (`StorySource`, `dispatch.rs`, validates it
/// and picks the source). Routes to the same fetch path a chapter fetch
/// already uses. A bad URL fails here, before any fetch starts. A pasted URL
/// is explicit intent to keep that story, so a successful import also
/// bookmarks it (`dispatch::bookmark_imported_story`) — always, with no
/// opt-out: this is the one place a story is bookmarked without the user
/// toggling the flag.
async fn add_story_by_url_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    url: String,
) -> Result<ChapterFetched, String> {
    let url = url.trim().to_string();
    let source = dispatch::story_url_source(&url)?;
    let fetched = fetch_by_source_via(env, db, session, source, url, Priority::Foreground).await?.into_chapter()?;
    dispatch::bookmark_imported_story(db, fetched.story_id)?;
    Ok(fetched)
}

#[tauri::command]
pub async fn add_story_by_url(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    url: String,
) -> Result<ChapterFetched, String> {
    add_story_by_url_via(&TauriEnv(&app), &db, &session, url).await
}

/// Fetches one chapter of an already-imported story — the source, URL and (AO3)
/// chapter id all come from the DB (`dispatch::chapter_fetch_url`), so the
/// frontend sends only `(story_id, chapter_num)`. Always fetches (no cache
/// check — callers ask the DB first). An unknown story fails here, before any
/// fetch starts. `background` marks bulk work (Download-all, bookmark refresh,
/// Reader's neighbour prefetch): it never supersedes a foreground FFN fetch
/// and fails with `BUSY` instead (AO3 fetches ignore it — no shared webview).
#[tauri::command]
pub async fn fetch_chapter(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    story_id: i64,
    chapter_num: i64,
    background: bool,
) -> Result<ChapterFetched, String> {
    fetch_chapter_via(&TauriEnv(&app), &db, &session, story_id, chapter_num, background).await
}

async fn fetch_chapter_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    story_id: i64,
    chapter_num: i64,
    background: bool,
) -> Result<ChapterFetched, String> {
    let (source, url) = dispatch::chapter_fetch_url(db, story_id, chapter_num)?;
    let priority = if background { Priority::Background } else { Priority::Foreground };
    fetch_by_source_via(env, db, session, source, url, priority).await?.into_chapter()
}

/// Makes one chapter of an already-imported story cached and returns it: the
/// stored body on a cache hit (no fetch, no `chapter_count`/`chapters`), else
/// a fetch (`fetch_chapter`'s path, same `background` rules and `BUSY`/
/// interrupted/timeout rejections) followed by the fresh chapter list and the
/// story's chapter count. Prefer this over `fetch_chapter` + read-back;
/// `fetch_chapter` stays for callers that must fetch unconditionally (the
/// debug page's bulk prefetch).
#[tauri::command]
pub async fn ensure_chapter(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    story_id: i64,
    chapter_num: i64,
    background: bool,
) -> Result<EnsuredChapter, String> {
    ensure_chapter_via(&TauriEnv(&app), &db, &session, story_id, chapter_num, background).await
}

async fn ensure_chapter_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    story_id: i64,
    chapter_num: i64,
    background: bool,
) -> Result<EnsuredChapter, String> {
    if let Some(hit) = dispatch::cached_chapter(db, story_id, chapter_num)? {
        return Ok(hit);
    }
    let fetched = fetch_chapter_via(env, db, session, story_id, chapter_num, background).await?;
    dispatch::ensured_after_fetch(db, story_id, chapter_num, &fetched)
}

/// FFN discovery (SPEC.md §6): fetches the next page of a browse/category
/// search. `category` is required for a brand-new fandom (the "Search FFN"
/// modal always has it); omit it for "Load more" on an already-known
/// fandom, where it's looked up from a prior search instead.
#[tauri::command]
pub async fn search_ffn(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    category: Option<String>,
    fandom: String,
    min_word_count: i64,
    status: ListingStatus,
) -> Result<BrowseFetched, String> {
    search_ffn_via(&TauriEnv(&app), &db, &session, category, fandom, min_word_count, status).await
}

async fn search_ffn_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    category: Option<String>,
    fandom: String,
    min_word_count: i64,
    status: ListingStatus,
) -> Result<BrowseFetched, String> {
    let (resolved_category, page) = db
        .run(|conn| db::next_browse_page(conn, category.as_deref(), &fandom, min_word_count, status.as_str()))?
        .ok_or_else(|| {
            format!("no category known for fandom {fandom:?} — search it from the Search FFN modal first")
        })?;
    let url = ffn::build_browse_url(&resolved_category, &fandom, min_word_count, status, page);
    // Stashed as-requested rather than left to be re-derived by decoding the
    // fetched URL later — see `FetchSession`.
    let locator = source::FfnListingLocator { category: resolved_category, fandom, min_word_count, status, page };
    fetch_ffn_via(env, db, session, url, Some(locator), Priority::Foreground).await?.into_browse()
}

/// AO3 counterpart to `search_ffn` — no category to resolve (AO3 has none),
/// and no webview step: `fetch_ao3_via` fetches and persists in one call (§8).
/// `sort_column` is hardcoded to `bookmarks_count` — the
/// closer analog to FFN's "always sort by Favorites for import relevance"
/// (§8 locked: AO3's bookmarks, not kudos, is the comparable deliberate
/// save/track signal to FFN's Favorites). `fandom` is force-applied to every
/// imported row (see `dispatch::persist_page`'s `fandom_override` param) so it
/// lands in the same fandom bucket as however this fandom is already named
/// on the FFN side — the whole point of searching both sources from one
/// Search screen, not two separate fandom lists (user-confirmed 2026-07-19).
#[tauri::command]
pub async fn search_ao3(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    tag: String,
    fandom: String,
    min_word_count: i64,
    status: ListingStatus,
) -> Result<BrowseFetched, String> {
    search_ao3_via(&TauriEnv(&app), &db, tag, fandom, min_word_count, status).await
}

async fn search_ao3_via(
    env: &impl FetchEnv,
    db: &db::Db,
    tag: String,
    fandom: String,
    min_word_count: i64,
    status: ListingStatus,
) -> Result<BrowseFetched, String> {
    let page = db.run(|conn| db::next_ao3_browse_page(conn, &tag, min_word_count, status.as_str()))?;
    let url = ao3::build_tag_browse_url(&tag, "bookmarks_count", min_word_count, status, page);
    let requested = source::Ao3ListingLocator { tag, min_word_count, status, page };
    fetch_ao3_via(env, db, &url, Some(&fandom), Some(requested)).await?.into_browse()
}

/// Fetches/refreshes a category's fandom list (SPEC.md §6 — precise
/// picker instead of free-text fandom entry). One page covers the whole
/// category (FFN doesn't paginate the index), so this always replaces the
/// cached list wholesale rather than needing "load more" semantics.
#[tauri::command]
pub async fn index_ffn_category(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    category: String,
) -> Result<FandomIndexFetched, String> {
    index_ffn_category_via(&TauriEnv(&app), &db, &session, category).await
}

async fn index_ffn_category_via(
    env: &impl FetchEnv,
    db: &db::Db,
    session: &FetchSession,
    category: String,
) -> Result<FandomIndexFetched, String> {
    fetch_ffn_via(env, db, session, format!("https://www.fanfiction.net/{category}/"), None, Priority::Foreground)
        .await?
        .into_fandom_index()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FFN_STORY_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_story_page.html"));
    const FFN_BROWSE_LISTING: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_browse_listing.html"));

    fn test_db() -> db::Db {
        db::Db(std::sync::Mutex::new(db::tests::memory_conn()))
    }

    #[test]
    fn persist_ffn_page_rejects_a_cloudflare_page_without_writing_anything() {
        let dbh = test_db();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

        let err = persist_ffn_page(&dbh, FFN_STORY_PAGE, url, "Just a moment...", None).unwrap_err();

        assert!(err.contains("Cloudflare"), "{err}");
        let stories: i64 = dbh.run(|c| c.query_row("SELECT COUNT(*) FROM stories", [], |r| r.get(0))).unwrap();
        assert_eq!(stories, 0);
    }

    #[test]
    fn persist_ffn_page_names_ffns_no_such_chapter_message_page() {
        let dbh = test_db();
        let html = r#"<html><body><div class="panel_normal"><span class="gui_normal">FanFiction.Net Message Type 1<hr>Story does not have any chapters. Please check to see you are not using an outdated url.</span></div></body></html>"#;

        let err = persist_ffn_page(&dbh, html, "https://www.fanfiction.net/s/14583034/20/", "", None).unwrap_err();

        assert!(err.contains("often temporary"), "{err}");
    }

    #[test]
    fn persist_ffn_page_persists_a_real_chapter_page() {
        let dbh = test_db();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

        let outcome =
            persist_ffn_page(&dbh, FFN_STORY_PAGE, url, "Welcome Chapter 34: Title | FanFiction", None).unwrap();

        assert_eq!(outcome.into_chapter().unwrap().chapter_num, 34);
    }

    #[test]
    fn persist_ffn_page_records_the_stashed_status_not_the_lossy_decoded_one() {
        let dbh = test_db();
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, crate::source::ListingStatus::InProgress, 1);
        let requested = source::FfnListingLocator {
            category: "anime".to_string(),
            fandom: "Fullmetal Alchemist".to_string(),
            min_word_count: 0,
            status: crate::source::ListingStatus::InProgress,
            page: 1,
        };

        persist_ffn_page(&dbh, FFN_BROWSE_LISTING, &url, "Fullmetal Alchemist | FanFiction", Some(requested)).unwrap();

        let next = dbh
            .run(|c| db::next_browse_page(c, None, "Fullmetal Alchemist", 0, "in_progress"))
            .unwrap();
        assert_eq!(next, Some(("anime".to_string(), 2)), "Load more must continue the in-progress search, not an 'any' one");
    }

    #[test]
    fn looks_blocked_true_for_each_known_cloudflare_title() {
        let clean_html = "<html><body>story prose</body></html>";
        assert!(looks_blocked(clean_html, "Just a moment..."));
        assert!(looks_blocked(clean_html, "Attention Required! | Cloudflare"));
        assert!(looks_blocked(clean_html, "Checking your browser before accessing..."));
    }

    #[test]
    fn looks_blocked_true_for_cf_browser_verification_marker_in_html() {
        let html = "<html><body><div id=\"cf-browser-verification\">...</div></body></html>";
        assert!(looks_blocked(html, "Some Story Chapter 1: Title | FanFiction"));
    }

    #[test]
    fn looks_blocked_false_for_a_real_chapter_title_and_html() {
        let html = "<html><body>Once upon a time, our hero set out.</body></html>";
        assert!(!looks_blocked(html, "Worth Dying For Chapter 1: Beginnings | FanFiction"));
    }

    #[test]
    fn looks_blocked_false_positive_on_a_fic_titled_with_a_trigger_phrase() {
        // Known, accepted risk (see looks_blocked's doc comment): a title-only
        // heuristic can't distinguish a Cloudflare interstitial from a real fic
        // that happens to share its title text.
        let html = "<html><body>story prose</body></html>";
        assert!(looks_blocked(html, "Checking Your Browser Chapter 1: A Fic | FanFiction"));
    }

    // ---- the fetch sequences, against a fake environment ----

    use crate::fetch_env::testing::{ok, status, FakeEnv, Report, SLUGGED_URL};
    use crate::fetch_session::{FetchSession, BUSY};
    use std::sync::Arc;

    const CHAPTER_URL: &str = "https://www.fanfiction.net/s/13493099/34/";
    const CHAPTER_TITLE: &str = "Welcome Chapter 34: Title | FanFiction";
    const CHALLENGE_HTML: &str = "<html><body>checking</body></html>";
    const CHALLENGE_TITLE: &str = "Just a moment...";

    const AO3_WORK_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ao3_work_page.html"));
    const AO3_LISTING_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ao3_listing_page.html"));
    const AO3_WORK_URL: &str = "https://archiveofourown.org/works/85954646";
    const AO3_CHAPTER_URL: &str = "https://archiveofourown.org/works/85954646/chapters/227238471";

    fn real_page() -> Report {
        Report { after_ms: 10, html: FFN_STORY_PAGE, title: CHAPTER_TITLE }
    }

    fn challenge(after_ms: u64) -> Report {
        Report { after_ms, html: CHALLENGE_HTML, title: CHALLENGE_TITLE }
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(f)
    }

    fn run_fetch(env: &FakeEnv, dbh: &db::Db, session: &FetchSession, priority: Priority) -> Result<FetchOutcome, String> {
        block_on(fetch_ffn_via(env, dbh, session, CHAPTER_URL.to_string(), None, priority))
    }

    fn fixtures() -> (db::Db, Arc<FetchSession>) {
        (test_db(), Arc::new(FetchSession::new()))
    }

    fn bookmarked(dbh: &db::Db, id: i64) -> bool {
        dbh.run(|conn| crate::queries::get_story(conn, id)).unwrap().unwrap().bookmarked
    }

    fn story_count(dbh: &db::Db) -> i64 {
        dbh.run(|c| c.query_row("SELECT COUNT(*) FROM stories", [], |r| r.get(0))).unwrap()
    }

    #[test]
    fn fetch_ffn_navigates_once_dumps_the_page_and_persists_the_chapter() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![real_page()]);

        let outcome = run_fetch(&env, &dbh, &session, Priority::Foreground).unwrap();

        assert_eq!(outcome.into_chapter().unwrap().chapter_num, 34);
        assert_eq!(*env.navigations.lock().unwrap(), vec![CHAPTER_URL.to_string()]);
        assert_eq!(env.dumps.lock().unwrap().len(), 1);
    }

    #[test]
    fn fetch_ffn_waits_for_the_real_page_after_a_challenge_page_without_navigating_again() {
        let (dbh, session) = fixtures();
        // Challenge first, then (well after the re-arm) the page it redirects to.
        let env = FakeEnv::new(&session, vec![challenge(10), Report { after_ms: 60, ..real_page() }]);

        let outcome = run_fetch(&env, &dbh, &session, Priority::Foreground).unwrap();

        assert_eq!(outcome.into_chapter().unwrap().chapter_num, 34);
        assert_eq!(env.navigations.lock().unwrap().len(), 1, "re-arm must not navigate again");
        // The dump is the real page, not the challenge.
        let dumps = env.dumps.lock().unwrap();
        assert_eq!(dumps.as_slice(), [("debug-last-fetch.html".to_string(), FFN_STORY_PAGE.to_string())]);
    }

    #[test]
    fn fetch_ffn_gives_up_with_the_cloudflare_error_after_the_re_arms_run_out() {
        let (dbh, session) = fixtures();
        // One initial report plus BLOCKED_REARMS more, all challenge pages.
        let reports = (0..=BLOCKED_REARMS).map(|i| challenge(if i == 0 { 10 } else { 60 })).collect();
        let env = FakeEnv::new(&session, reports);

        let err = run_fetch(&env, &dbh, &session, Priority::Foreground).unwrap_err();

        assert!(err.contains("Cloudflare"), "{err}");
        assert_eq!(story_count(&dbh), 0);
    }

    #[test]
    fn fetch_ffn_times_out_when_a_challenge_page_is_never_followed_by_the_real_one() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![challenge(10)]);

        let err = run_fetch(&env, &dbh, &session, Priority::Foreground).unwrap_err();

        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn fetch_ffn_refuses_a_background_fetch_while_a_foreground_one_is_pending_without_navigating() {
        let (dbh, session) = fixtures();
        let _foreground = session.start("https://www.fanfiction.net/s/1/1/", None);
        let env = FakeEnv::new(&session, vec![real_page()]);

        let err = run_fetch(&env, &dbh, &session, Priority::Background).unwrap_err();

        assert_eq!(err, BUSY);
        assert!(env.navigations.lock().unwrap().is_empty());
    }

    #[test]
    fn fetch_ffn_fails_and_frees_the_slot_when_navigation_fails() {
        let (dbh, session) = fixtures();
        let mut env = FakeEnv::new(&session, vec![]);
        env.navigate_error = Some("window gone".to_string());

        let err = run_fetch(&env, &dbh, &session, Priority::Foreground).unwrap_err();

        assert_eq!(err, "window gone");
        // Nothing left pending: a late report is dropped, and a new fetch can start.
        assert!(!session.deliver(SLUGGED_URL, String::new(), String::new()));
    }

    // ---- add_story_by_url_via ----

    #[test]
    fn add_story_by_url_imports_and_bookmarks_an_ffn_story() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![real_page()]);

        let fetched = block_on(add_story_by_url_via(&env, &dbh, &session, format!(" {CHAPTER_URL} "))).unwrap();

        assert!(bookmarked(&dbh, fetched.story_id));
        assert_eq!(*env.navigations.lock().unwrap(), vec![CHAPTER_URL.to_string()], "the URL is trimmed");
    }

    #[test]
    fn add_story_by_url_imports_and_bookmarks_an_ao3_story_through_the_http_path() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::ao3(vec![ok(AO3_WORK_PAGE, AO3_CHAPTER_URL)]);

        let fetched = block_on(add_story_by_url_via(&env, &dbh, &session, AO3_WORK_URL.to_string())).unwrap();

        assert!(bookmarked(&dbh, fetched.story_id));
        assert_eq!(*env.gets.lock().unwrap(), vec![AO3_WORK_URL.to_string()]);
        assert!(env.navigations.lock().unwrap().is_empty(), "AO3 never touches the webview");
        assert_eq!(env.dumps.lock().unwrap()[0].0, "debug-last-ao3-fetch.html");
    }

    #[test]
    fn add_story_by_url_rejects_a_bad_url_before_any_fetch() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::ao3(vec![]);

        let err = block_on(add_story_by_url_via(&env, &dbh, &session, "https://example.com/x".to_string())).unwrap_err();

        assert!(err.contains("only fanfiction.net and archiveofourown.org"), "{err}");
        assert!(env.gets.lock().unwrap().is_empty() && env.navigations.lock().unwrap().is_empty());
    }

    #[test]
    fn add_story_by_url_does_not_bookmark_when_the_fetch_fails() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::ao3(vec![status(404)]);

        let err = block_on(add_story_by_url_via(&env, &dbh, &session, AO3_WORK_URL.to_string())).unwrap_err();

        assert!(err.contains("404"), "{err}");
        assert_eq!(story_count(&dbh), 0);
    }

    // ---- ensure_chapter_via / fetch_chapter_via ----

    /// An FFN story imported through the fetch path, then its chapter 34 body
    /// blanked back to a TOC stub, so the next `ensure_chapter` has to fetch.
    fn imported_story_with_chapter_34_uncached(env: &FakeEnv, dbh: &db::Db, session: &FetchSession) -> i64 {
        let fetched = block_on(add_story_by_url_via(env, dbh, session, CHAPTER_URL.to_string())).unwrap();
        dbh.run(|c| {
            c.execute("UPDATE chapters SET content = '' WHERE story_id = ?1 AND chapter_num = 34", [fetched.story_id])
        })
        .unwrap();
        env.navigations.lock().unwrap().clear();
        fetched.story_id
    }

    #[test]
    fn ensure_chapter_fetches_on_a_miss_and_then_serves_the_cache_without_fetching() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![real_page()]);
        let story_id = imported_story_with_chapter_34_uncached(&env, &dbh, &session);

        let first = block_on(ensure_chapter_via(&env, &dbh, &session, story_id, 34, false)).unwrap();
        let second = block_on(ensure_chapter_via(&env, &dbh, &session, story_id, 34, false)).unwrap();

        assert!(first.was_fetched && first.content.is_cached);
        assert_eq!(first.chapters.unwrap().len() as i64, first.chapter_count.unwrap());
        assert!(!second.was_fetched && second.chapters.is_none());
        assert_eq!(env.navigations.lock().unwrap().len(), 1, "only the miss navigates");
    }

    #[test]
    fn ensure_chapter_background_never_supersedes_a_pending_foreground_fetch() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![real_page()]);
        let story_id = imported_story_with_chapter_34_uncached(&env, &dbh, &session);
        let _foreground = session.start("https://www.fanfiction.net/s/1/1/", None);

        let err = block_on(ensure_chapter_via(&env, &dbh, &session, story_id, 34, true)).unwrap_err();

        assert_eq!(err, BUSY);
        assert!(env.navigations.lock().unwrap().is_empty());
    }

    #[test]
    fn ensure_chapter_foreground_supersedes_nothing_it_just_fetches() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![real_page()]);
        let story_id = imported_story_with_chapter_34_uncached(&env, &dbh, &session);

        let ensured = block_on(ensure_chapter_via(&env, &dbh, &session, story_id, 34, false)).unwrap();

        assert!(ensured.was_fetched);
    }

    #[test]
    fn fetch_chapter_for_an_unknown_story_fails_before_any_fetch() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![real_page()]);

        let err = block_on(fetch_chapter_via(&env, &dbh, &session, 999, 1, false)).unwrap_err();

        assert_eq!(err, "no story with id 999");
        assert!(env.navigations.lock().unwrap().is_empty());
    }

    #[test]
    fn an_ao3_chapter_is_fetched_by_the_url_stored_for_it() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::ao3(vec![ok(AO3_WORK_PAGE, AO3_CHAPTER_URL), ok(AO3_WORK_PAGE, AO3_CHAPTER_URL)]);
        let fetched = block_on(add_story_by_url_via(&env, &dbh, &session, AO3_WORK_URL.to_string())).unwrap();

        let again = block_on(fetch_chapter_via(&env, &dbh, &session, fetched.story_id, fetched.chapter_num, true)).unwrap();

        assert_eq!(again.story_id, fetched.story_id);
        assert_eq!(env.gets.lock().unwrap()[1], AO3_CHAPTER_URL, "the stored chapter id builds the URL");
    }

    #[test]
    fn an_ao3_fetch_ignores_background_priority() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::ao3(vec![ok(AO3_WORK_PAGE, AO3_CHAPTER_URL), ok(AO3_WORK_PAGE, AO3_CHAPTER_URL)]);
        let fetched = block_on(add_story_by_url_via(&env, &dbh, &session, AO3_WORK_URL.to_string())).unwrap();
        let _foreground = session.start("https://www.fanfiction.net/s/1/1/", None);

        let again = block_on(fetch_chapter_via(&env, &dbh, &session, fetched.story_id, fetched.chapter_num, true));

        assert!(again.is_ok(), "{again:?}");
    }

    // ---- search commands ----

    #[test]
    fn search_ao3_imports_a_listing_under_the_forced_fandom_and_load_more_asks_for_page_two() {
        let (dbh, _session) = fixtures();
        let page = |n| ao3::build_tag_browse_url("Naruto", "bookmarks_count", 0, ListingStatus::Any, n);
        let env = FakeEnv::ao3(vec![ok(AO3_LISTING_PAGE, &page(1)), ok(AO3_LISTING_PAGE, &page(2))]);
        let search = || {
            block_on(search_ao3_via(&env, &dbh, "Naruto".to_string(), "Naruto (forced)".to_string(), 0, ListingStatus::Any))
        };

        let first = search().unwrap();
        let second = search().unwrap();

        assert!(first.count > 0 && second.count > 0, "{first:?} {second:?}");
        assert_eq!(*env.gets.lock().unwrap(), vec![page(1), page(2)], "Load more continues at page 2");
        let forced: i64 = dbh
            .run(|c| c.query_row("SELECT COUNT(*) FROM stories WHERE fandom = 'Naruto (forced)'", [], |r| r.get(0)))
            .unwrap();
        assert!(forced > 0);
    }

    #[test]
    fn search_ffn_without_a_known_category_fails_before_any_fetch() {
        let (dbh, session) = fixtures();
        let env = FakeEnv::new(&session, vec![]);

        let err = block_on(search_ffn_via(&env, &dbh, &session, None, "Naruto".to_string(), 0, ListingStatus::Any))
            .unwrap_err();

        assert!(err.contains("no category known"), "{err}");
        assert!(env.navigations.lock().unwrap().is_empty());
    }

    #[test]
    fn search_ffn_stashes_the_requested_locator_so_load_more_continues_the_same_search() {
        let (dbh, session) = fixtures();
        let mut env = FakeEnv::new(
            &session,
            vec![Report { after_ms: 10, html: FFN_BROWSE_LISTING, title: "Fullmetal Alchemist | FanFiction" }],
        );
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, ListingStatus::InProgress, 1);
        env.report_url = url.clone();

        block_on(search_ffn_via(
            &env,
            &dbh,
            &session,
            Some("anime".to_string()),
            "Fullmetal Alchemist".to_string(),
            0,
            ListingStatus::InProgress,
        ))
        .unwrap();

        assert_eq!(*env.navigations.lock().unwrap(), vec![url]);
        let next = dbh.run(|c| db::next_browse_page(c, None, "Fullmetal Alchemist", 0, "in_progress")).unwrap();
        assert_eq!(next, Some(("anime".to_string(), 2)));
    }

    #[test]
    fn index_ffn_category_fetches_the_category_index_page() {
        let (dbh, session) = fixtures();
        let mut env = FakeEnv::new(
            &session,
            vec![Report {
                after_ms: 10,
                html: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_category_index.html")),
                title: "Anime Crossover | FanFiction",
            }],
        );
        env.report_url = "https://www.fanfiction.net/anime/".to_string();

        let indexed = block_on(index_ffn_category_via(&env, &dbh, &session, "anime".to_string())).unwrap();

        assert_eq!(*env.navigations.lock().unwrap(), vec!["https://www.fanfiction.net/anime/".to_string()]);
        assert!(indexed.count > 0, "{indexed:?}");
    }
}
