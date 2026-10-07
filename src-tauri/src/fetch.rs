//! Fetch orchestration: driving `FicSource` (`source.rs`) dispatch against
//! the two concrete fetch mechanics (FFN's webview round-trip, AO3's direct
//! HTTP call). Every fetch command returns its parsed-and-persisted result
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

use crate::dispatch::{self, BrowseFetched, ChapterFetched, FandomIndexFetched, FetchOutcome, StoryUrlSource};
use crate::fetch_session::{FetchSession, Priority, FETCH_TIMEOUT};
use crate::source::{self, ListingStatus};
use crate::{ao3, db, ffn};
use tauri::Manager;

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

/// Writes the raw page to `<app data dir>/<file_name>` so a failed parse or a
/// false-positive block check can still be inspected. Best-effort: a
/// missing data dir or a write error must never fail the fetch.
fn dump_last_fetch(app: &tauri::AppHandle, file_name: &str, html: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::write(dir.join(file_name), html);
    }
}

/// FFN-specific pre-check (Cloudflare-block detection), then the shared
/// `dispatch::persist_page::<source::Ffn>` — which also owns FFN's
/// category-index shape, so there is no FFN-only branch left here. Free of
/// `AppHandle` (the debug dump lives in `fetch_ffn`) so it runs from fixtures.
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

/// How many times `fetch_ffn` waits again after a Cloudflare page is reported.
const BLOCKED_REARMS: u32 = 2;

/// Points the single reused `fetch-page` window at `url`, building it on the
/// app's first fetch. Tauri's own on_page_load(Finished) turned out
/// unreliable on some FFN pages (browse/category pages in particular) —
/// never fired even though the page visibly rendered correctly, likely
/// confused by a subframe (ad/tracking iframe) navigating independently.
/// Triggering off the page's own native `load` event instead, via an init
/// script that runs before the page's own scripts on every navigation,
/// tracks what's actually on screen rather than Tauri's navigation-delegate
/// state. The init script and `on_navigation` below apply to every
/// subsequent `navigate()` call on this same webview, not just the first load.
fn open_or_navigate(app: &tauri::AppHandle, parsed: url::Url) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("fetch-page") {
        println!("[fetch] navigating existing window to {parsed}");
        existing.navigate(parsed).map_err(|e| e.to_string())?;
        return Ok(());
    }

    println!("[fetch] opening window for {parsed}");

    tauri::WebviewWindowBuilder::new(app, "fetch-page", tauri::WebviewUrl::External(parsed))
        .title("Fetching… (watch for a Cloudflare challenge)")
        .inner_size(900.0, 700.0)
        // Hidden by default — the window flashing on screen for every
        // chapter fetch/prefetch was disruptive. Caveat: if fanfiction.net
        // ever serves an interactive Cloudflare challenge (rare once
        // cf_clearance is established), there's no visible window to solve
        // it in and the fetch will just time out. Temporarily flip this to
        // `true` to debug that case.
        .visible(false)
        .focused(false)
        .on_navigation(|url| {
            println!("[fetch] on_navigation: {url}");
            true
        })
        .initialization_script(include_str!("fetch_page_init.js"))
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// What `fetch_ffn_via` needs from the app's hidden `fetch-page` webview —
/// the seam between the fetch sequence (testable) and Tauri (not). The real
/// adapter is `TauriWebview`; tests use a fake that reports pages to the
/// session the way `fetch_page_init.js` does.
trait FfnWebview {
    /// Points the webview at `url` (building it on the app's first fetch).
    fn navigate(&self, url: url::Url) -> Result<(), String>;
    /// Best-effort copy of the page for inspection (`dump_last_fetch`).
    fn save_debug_page(&self, file_name: &str, html: &str);
}

struct TauriWebview<'a>(&'a tauri::AppHandle);

impl FfnWebview for TauriWebview<'_> {
    fn navigate(&self, url: url::Url) -> Result<(), String> {
        open_or_navigate(self.0, url)
    }

    fn save_debug_page(&self, file_name: &str, html: &str) {
        dump_last_fetch(self.0, file_name, html);
    }
}

/// One FFN fetch end to end: navigate the webview, wait for the session to
/// deliver the rendered page (or reject: interrupted, timed out), persist it.
async fn fetch_ffn(
    app: &tauri::AppHandle,
    db: &db::Db,
    session: &FetchSession,
    url: String,
    listing_locator: Option<source::FfnListingLocator>,
    priority: Priority,
) -> Result<FetchOutcome, String> {
    fetch_ffn_via(&TauriWebview(app), db, session, url, listing_locator, priority, FETCH_TIMEOUT).await
}

/// `fetch_ffn` with the webview and the per-wait timeout injected, so the
/// whole sequence — priority refusal, navigation failure, Cloudflare re-arm,
/// timeout, persist — runs from tests.
async fn fetch_ffn_via(
    webview: &impl FfnWebview,
    db: &db::Db,
    session: &FetchSession,
    url: String,
    listing_locator: Option<source::FfnListingLocator>,
    priority: Priority,
    timeout: std::time::Duration,
) -> Result<FetchOutcome, String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;

    let fetch = {
        let _window = session.lock_window().await;
        // A refused background fetch returns before touching the webview.
        let fetch = match priority {
            Priority::Foreground => session.start(&url, listing_locator),
            Priority::Background => session.start_background(&url, listing_locator)?,
        };
        if let Err(e) = webview.navigate(parsed) {
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
    webview.save_debug_page("debug-last-fetch.html", &page.html);
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
/// rejects after `FETCH_TIMEOUT`. `listing_locator` is the exact locator
/// `search_ffn` built its URL from — see `FetchSession`.
#[tauri::command]
pub async fn fetch_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    url: String,
    listing_locator: Option<source::FfnListingLocator>,
) -> Result<FetchOutcome, String> {
    fetch_ffn(&app, &db, &session, url, listing_locator, Priority::Foreground).await
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

// ---- AO3 fetch path (SPEC.md §8) ----
//
// Deliberately not a copy of `fetch_page`'s webview/window/event-callback
// machinery: that whole apparatus exists only to get past Cloudflare (§3),
// and a bare HTTP client *is* sufficient for AO3 — no webview, no cookies,
// no JS needed. So this fetches and persists in one direct async call
// rather than firing a window and waiting for a `report_fetch_result`
// round-trip.
//
// **Revises §8's "no bot-fingerprinting" finding**: re-tested live
// (2026-07-19, this session) with `reqwest`'s and `urllib`'s *default*
// User-Agent strings and got a real Cloudflare "managed challenge" (503
// wrapper page, "Shields are up!") on both a work page and a tag listing —
// not the flakiness §8 already documented, a genuine block. Setting a
// realistic desktop-browser User-Agent header made both fetch clean on the
// first try, confirmed with both `urllib` and `reqwest` directly. So AO3's
// Cloudflare *is* doing UA-based filtering, just not full TLS/JS
// fingerprinting the way FFN's does (§3) — a static UA header clears it,
// no webview/interactive-challenge machinery required.
//
// What AO3's origin *does* still need, confirmed live (§8), is
// retry-with-backoff: a handful of bare requests hit plain `TimeoutError`s
// and an `HTTP 525` (Cloudflare origin SSL error) before a retry succeeded,
// with no challenge page involved — ordinary origin flakiness, distinct
// from the UA-based block above. FFN's fetch loop never needed retries,
// only inter-request pacing (§3); AO3's needs the opposite.

const AO3_FETCH_MAX_ATTEMPTS: u32 = 4;
const AO3_FETCH_TIMEOUT_SECS: u64 = 20;

// A real desktop-browser UA — confirmed live (above) necessary to avoid a
// Cloudflare "managed challenge" 403 that reqwest's/urllib's own default UA
// strings both trigger.
const AO3_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/// Fetches a single AO3 URL, retrying on connection/timeout errors and 5xx
/// responses (the flakiness confirmed live in SPEC.md §8) with exponential
/// backoff (500ms, 1s, 2s, ...). Does *not* retry on 4xx — a bad tag/work id
/// (or a Cloudflare challenge the UA header above didn't clear) isn't going
/// to fix itself. Returns the final post-redirect URL alongside the body:
/// AO3 redirects a bare `/works/{id}` to that work's actual first chapter
/// URL, and callers need the URL that was *actually* served to know which
/// chapter this HTML is.
async fn fetch_ao3_html(url: &str) -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(AO3_FETCH_TIMEOUT_SECS))
        .user_agent(AO3_USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let mut last_err = String::new();
    for attempt in 1..=AO3_FETCH_MAX_ATTEMPTS {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_server_error() => {
                last_err = format!("HTTP {}", resp.status());
            }
            Ok(resp) if !resp.status().is_success() => {
                return Err(format!(
                    "AO3 returned HTTP {} (not a server error, won't retry — possibly a Cloudflare challenge or a bad url)",
                    resp.status()
                ));
            }
            Ok(resp) => {
                let final_url = resp.url().to_string();
                return resp
                    .text()
                    .await
                    .map(|html| (html, final_url))
                    .map_err(|e| format!("failed reading response body: {e}"));
            }
            Err(e) if e.is_timeout() || e.is_connect() => {
                last_err = e.to_string();
            }
            Err(e) => return Err(e.to_string()),
        }

        if attempt < AO3_FETCH_MAX_ATTEMPTS {
            let backoff_ms = 500u64 * 2u64.pow(attempt - 1);
            println!("[ao3-fetch] attempt {attempt}/{AO3_FETCH_MAX_ATTEMPTS} failed ({last_err}), retrying in {backoff_ms}ms");
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        }
    }
    Err(format!("AO3 fetch failed after {AO3_FETCH_MAX_ATTEMPTS} attempts: {last_err}"))
}

/// AO3 counterpart to `fetch_ffn` — see the module note above on why this
/// doesn't need the webview/session machinery. `fandom_override`, when
/// given, force-applies that label to every row of a tag-listing fetch
/// (ignored for a work/chapter fetch) — see `source::Ao3::apply_fandom_override`.
async fn fetch_ao3(
    app: &tauri::AppHandle,
    db: &db::Db,
    url: &str,
    fandom_override: Option<&str>,
) -> Result<FetchOutcome, String> {
    let (html, final_url) = fetch_ao3_html(url).await.inspect_err(|e| eprintln!("[ao3-fetch] failed: {e}"))?;
    // Dumped unconditionally, same posture as the FFN path's own
    // debug-last-fetch.html — a failed parse should still be inspectable.
    dump_last_fetch(app, "debug-last-ao3-fetch.html", &html);
    dispatch::persist_page::<source::Ao3>(db, &html, &final_url, fandom_override, None)
        .inspect_err(|e| eprintln!("[ao3-fetch] failed: {e}"))
}

#[tauri::command]
pub async fn fetch_ao3_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    url: String,
    fandom_override: Option<String>,
) -> Result<FetchOutcome, String> {
    fetch_ao3(&app, &db, &url, fandom_override.as_deref()).await
}

/// Fetches `url` through whichever mechanism its source needs.
async fn fetch_by_source(
    app: &tauri::AppHandle,
    db: &db::Db,
    session: &FetchSession,
    source: StoryUrlSource,
    url: String,
    priority: Priority,
) -> Result<FetchOutcome, String> {
    match source {
        StoryUrlSource::Ffn => fetch_ffn(app, db, session, url, None, priority).await,
        StoryUrlSource::Ao3 => fetch_ao3(app, db, &url, None).await,
    }
}

/// Adds one story by pasted URL (`StoryUrlSource`, `dispatch.rs`, validates it
/// and picks the source). Routes to the same fetch path a chapter fetch
/// already uses. A bad URL fails here, before any fetch starts.
#[tauri::command]
pub async fn add_story_by_url(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    session: tauri::State<'_, FetchSession>,
    url: String,
) -> Result<ChapterFetched, String> {
    let url = url.trim().to_string();
    let source = dispatch::story_url_source(&url)?;
    fetch_by_source(&app, &db, &session, source, url, Priority::Foreground).await?.into_chapter()
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
    let (source, url) = dispatch::chapter_fetch_url(&db, story_id, chapter_num)?;
    let priority = if background { Priority::Background } else { Priority::Foreground };
    fetch_by_source(&app, &db, &session, source, url, priority).await?.into_chapter()
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
    let (resolved_category, page) = db
        .run(|conn| db::next_browse_page(conn, category.as_deref(), &fandom, min_word_count, status.as_str()))?
        .ok_or_else(|| {
            format!("no category known for fandom {fandom:?} — search it from the Search FFN modal first")
        })?;
    let url = ffn::build_browse_url(&resolved_category, &fandom, min_word_count, status, page);
    // Stashed as-requested rather than left to be re-derived by decoding the
    // fetched URL later — see `FetchSession`.
    let locator = source::FfnListingLocator { category: resolved_category, fandom, min_word_count, status, page };
    fetch_ffn(&app, &db, &session, url, Some(locator), Priority::Foreground).await?.into_browse()
}

/// AO3 counterpart to `search_ffn` — no category to resolve (AO3 has none),
/// and no webview step: `fetch_ao3` fetches and persists in one call (§8).
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
    let page = db.run(|conn| db::next_ao3_browse_page(conn, &tag, min_word_count, status.as_str()))?;
    let url = ao3::build_tag_browse_url(&tag, "bookmarks_count", min_word_count, status, page);
    fetch_ao3(&app, &db, &url, Some(&fandom)).await?.into_browse()
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
    fetch_ffn(&app, &db, &session, format!("https://www.fanfiction.net/{category}/"), None, Priority::Foreground)
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

    // ---- fetch_ffn_via: the whole sequence against a fake webview ----

    use crate::fetch_session::{FetchSession, BUSY};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const CHAPTER_URL: &str = "https://www.fanfiction.net/s/13493099/34/";
    const SLUGGED_URL: &str = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";
    const CHAPTER_TITLE: &str = "Welcome Chapter 34: Title | FanFiction";
    const CHALLENGE_HTML: &str = "<html><body>checking</body></html>";
    const CHALLENGE_TITLE: &str = "Just a moment...";
    const WAIT: Duration = Duration::from_millis(400);

    /// One report the fake webview makes after `navigate`, `after` the
    /// previous one — what `fetch_page_init.js` does on each DOMContentLoaded.
    struct Report {
        after_ms: u64,
        html: &'static str,
        title: &'static str,
    }

    struct FakeWebview {
        session: Arc<FetchSession>,
        reports: Vec<Report>,
        navigate_error: Option<String>,
        navigations: Mutex<Vec<String>>,
        dumps: Mutex<Vec<String>>,
    }

    impl FakeWebview {
        fn new(session: &Arc<FetchSession>, reports: Vec<Report>) -> Self {
            Self {
                session: Arc::clone(session),
                reports,
                navigate_error: None,
                navigations: Mutex::new(vec![]),
                dumps: Mutex::new(vec![]),
            }
        }
    }

    impl FfnWebview for FakeWebview {
        fn navigate(&self, url: url::Url) -> Result<(), String> {
            self.navigations.lock().unwrap().push(url.to_string());
            if let Some(e) = &self.navigate_error {
                return Err(e.clone());
            }
            let session = Arc::clone(&self.session);
            let script: Vec<(u64, &'static str, &'static str)> =
                self.reports.iter().map(|r| (r.after_ms, r.html, r.title)).collect();
            tokio::spawn(async move {
                for (after_ms, html, title) in script {
                    tokio::time::sleep(Duration::from_millis(after_ms)).await;
                    session.deliver(SLUGGED_URL, html.to_string(), title.to_string());
                }
            });
            Ok(())
        }

        fn save_debug_page(&self, _file_name: &str, html: &str) {
            self.dumps.lock().unwrap().push(html.to_string());
        }
    }

    fn real_page() -> Report {
        Report { after_ms: 10, html: FFN_STORY_PAGE, title: CHAPTER_TITLE }
    }

    fn challenge(after_ms: u64) -> Report {
        Report { after_ms, html: CHALLENGE_HTML, title: CHALLENGE_TITLE }
    }

    fn run_fetch(
        webview: &FakeWebview,
        dbh: &db::Db,
        session: &FetchSession,
        priority: Priority,
    ) -> Result<FetchOutcome, String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(fetch_ffn_via(webview, dbh, session, CHAPTER_URL.to_string(), None, priority, WAIT))
    }

    #[test]
    fn fetch_ffn_navigates_once_dumps_the_page_and_persists_the_chapter() {
        let (dbh, session) = (test_db(), Arc::new(FetchSession::new()));
        let webview = FakeWebview::new(&session, vec![real_page()]);

        let outcome = run_fetch(&webview, &dbh, &session, Priority::Foreground).unwrap();

        assert_eq!(outcome.into_chapter().unwrap().chapter_num, 34);
        assert_eq!(*webview.navigations.lock().unwrap(), vec![CHAPTER_URL.to_string()]);
        assert_eq!(webview.dumps.lock().unwrap().len(), 1);
    }

    #[test]
    fn fetch_ffn_waits_for_the_real_page_after_a_challenge_page_without_navigating_again() {
        let (dbh, session) = (test_db(), Arc::new(FetchSession::new()));
        // Challenge first, then (well after the re-arm) the page it redirects to.
        let webview = FakeWebview::new(&session, vec![challenge(10), Report { after_ms: 60, ..real_page() }]);

        let outcome = run_fetch(&webview, &dbh, &session, Priority::Foreground).unwrap();

        assert_eq!(outcome.into_chapter().unwrap().chapter_num, 34);
        assert_eq!(webview.navigations.lock().unwrap().len(), 1, "re-arm must not navigate again");
        // The dump is the real page, not the challenge.
        assert_eq!(webview.dumps.lock().unwrap().as_slice(), [FFN_STORY_PAGE.to_string()]);
    }

    #[test]
    fn fetch_ffn_gives_up_with_the_cloudflare_error_after_the_re_arms_run_out() {
        let (dbh, session) = (test_db(), Arc::new(FetchSession::new()));
        // One initial report plus BLOCKED_REARMS more, all challenge pages.
        let reports = (0..=BLOCKED_REARMS).map(|i| challenge(if i == 0 { 10 } else { 60 })).collect();
        let webview = FakeWebview::new(&session, reports);

        let err = run_fetch(&webview, &dbh, &session, Priority::Foreground).unwrap_err();

        assert!(err.contains("Cloudflare"), "{err}");
        let stories: i64 = dbh.run(|c| c.query_row("SELECT COUNT(*) FROM stories", [], |r| r.get(0))).unwrap();
        assert_eq!(stories, 0);
    }

    #[test]
    fn fetch_ffn_times_out_when_a_challenge_page_is_never_followed_by_the_real_one() {
        let (dbh, session) = (test_db(), Arc::new(FetchSession::new()));
        let webview = FakeWebview::new(&session, vec![challenge(10)]);

        let err = run_fetch(&webview, &dbh, &session, Priority::Foreground).unwrap_err();

        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn fetch_ffn_refuses_a_background_fetch_while_a_foreground_one_is_pending_without_navigating() {
        let (dbh, session) = (test_db(), Arc::new(FetchSession::new()));
        let _foreground = session.start("https://www.fanfiction.net/s/1/1/", None);
        let webview = FakeWebview::new(&session, vec![real_page()]);

        let err = run_fetch(&webview, &dbh, &session, Priority::Background).unwrap_err();

        assert_eq!(err, BUSY);
        assert!(webview.navigations.lock().unwrap().is_empty());
    }

    #[test]
    fn fetch_ffn_fails_and_frees_the_slot_when_navigation_fails() {
        let (dbh, session) = (test_db(), Arc::new(FetchSession::new()));
        let mut webview = FakeWebview::new(&session, vec![]);
        webview.navigate_error = Some("window gone".to_string());

        let err = run_fetch(&webview, &dbh, &session, Priority::Foreground).unwrap_err();

        assert_eq!(err, "window gone");
        // Nothing left pending: a late report is dropped, and a new fetch can start.
        assert!(!session.deliver(SLUGGED_URL, String::new(), String::new()));
    }
}
