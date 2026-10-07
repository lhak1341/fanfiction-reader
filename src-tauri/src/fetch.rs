//! Fetch orchestration: driving `FicSource` (`source.rs`) dispatch against
//! the two concrete fetch mechanics (FFN's webview round-trip, AO3's direct
//! HTTP call), turning either into `chapter-fetched`/`browse-fetched`/
//! `fandom-index-fetched`/error events. `source.rs`'s own doc comment
//! (architecture review candidate 1, 2026-07-19) already establishes that
//! the trait lives there and the dispatch driver lives outside it — this
//! module is that driver's home, split out of `lib.rs` (architecture
//! review candidate, 2026-08-05) so command registration in `lib.rs` stays
//! a thin list instead of interleaving with ~750 lines of fetch machinery.

use crate::dispatch::{self, FetchOutcome, FetchTarget};
use crate::source::{self, FicSource};
use crate::{ao3, db, ffn};
use tauri::{Emitter, Manager};

/// A fetch in flight: the URL it was issued for, plus — for an FFN listing
/// fetch only — the exact locator `search_ffn` built its URL from.
/// Architecture review candidate 1 (2026-07-29): stashed here instead of
/// re-derived by decoding the *fetched* page's URL once it comes back,
/// because `build_browse_url`'s encoding is lossy for `status` (`ffn.rs`) —
/// decoding can't tell `"in_progress"` apart from `"any"`, which used to
/// desync `record_listing_page`'s write from `next_browse_page`'s lookup and
/// permanently break "Load more" for any in-progress-scoped FFN search.
pub(crate) struct PendingFetch {
    url: String,
    listing_locator: Option<source::FfnListingLocator>,
}

/// Serializes access to the single reused `fetch-page` window (see below) —
/// background chapter prefetching can now overlap in time with a foreground
/// navigation fetch, and without this two concurrent `fetch_page` calls can
/// both see the old window, both try to tear it down and rebuild it under
/// the same label, and one loses with "a webview with label `fetch-page`
/// already exists". Also tracks *which* URL is currently loading: a
/// `navigate()` that interrupts a still-pending one would otherwise leave
/// that pending fetch's `report_fetch_result` never invoked — its awaiter
/// just hangs forever, silently, since the interrupted page's own
/// `DOMContentLoaded`/timeout listeners die with the navigation. `fetch_page`
/// checks this before overwriting it, so the interrupted fetch gets a real
/// error the instant it's actually interrupted, not a silent hang.
pub struct FetchLock(pub tokio::sync::Mutex<Option<PendingFetch>>);

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
#[tauri::command]
pub async fn fetch_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    url: String,
    listing_locator: Option<source::FfnListingLocator>,
) -> Result<(), String> {
    let lock = app.state::<FetchLock>();
    let mut pending = lock.0.lock().await;

    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;

    if let Some(interrupted) = pending.replace(PendingFetch { url: url.clone(), listing_locator }) {
        let target = resolve_interrupted_target(&interrupted, &db);
        emit_targeted_error(&app, target, "interrupted by a newer fetch".to_string());
    }

    if let Some(existing) = app.get_webview_window("fetch-page") {
        println!("[fetch] navigating existing window to {parsed}");
        existing.navigate(parsed).map_err(|e| e.to_string())?;
        return Ok(());
    }

    println!("[fetch] opening window for {parsed}");

    // Tauri's own on_page_load(Finished) turned out unreliable on some FFN
    // pages (browse/category pages in particular) — never fired even though
    // the page visibly rendered correctly, likely confused by a subframe
    // (ad/tracking iframe) navigating independently. Triggering off the
    // page's own native `load` event instead, via an init script that runs
    // before the page's own scripts on every navigation, tracks what's
    // actually on screen rather than Tauri's navigation-delegate state.
    // The init script and `on_navigation` below apply to every subsequent
    // `navigate()` call on this same webview, not just this first load.
    tauri::WebviewWindowBuilder::new(&app, "fetch-page", tauri::WebviewUrl::External(parsed))
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

/// Error twins of the three `*Fetched` structs above, carrying the same
/// identifying fields as their success counterpart plus `message` — lets
/// `awaitCorrelatedEvent` (`tauriEvents.ts`) use one `match` predicate across
/// both its success and error listener, instead of an error resolving
/// whichever call happens to be waiting regardless of which fetch it was.
#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChapterFetchError {
    story_id: i64,
    chapter_num: i64,
    message: String,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BrowseFetchError {
    fandom: String,
    message: String,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FandomIndexFetchError {
    category: String,
    message: String,
}

/// `FetchTarget::Unknown`'s payload shape (`emit_targeted_error`) — same
/// `{ message }` wire shape as the three typed errors above, just without
/// their identifying fields, so the TS side never has to branch on "is this
/// payload a bare string or an object" to read `.message` back out.
#[derive(Debug, serde::Serialize, Clone)]
struct UnmatchedFetchError {
    message: String,
}

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

/// FFN-specific pre-checks (debug dump, Cloudflare-block detection), then the
/// shared `dispatch::persist_page::<source::Ffn>` — which also owns FFN's
/// category-index shape, so there is no FFN-only branch left here.
fn try_persist_ffn_fetch(
    app: &tauri::AppHandle,
    db: &tauri::State<db::Db>,
    html: &str,
    page_url: &str,
    title: &str,
    listing_locator: Option<source::FfnListingLocator>,
) -> Result<FetchOutcome, String> {
    // Dumped to disk unconditionally (before the blocked check) so a false
    // positive there can still be inspected, not just a genuine block.
    let dump_path = app.path().app_data_dir().expect("resolve app data dir").join("debug-last-fetch.html");
    let _ = std::fs::write(&dump_path, html);

    if looks_blocked(html, title) {
        return Err("Cloudflare challenge page, not the story — try again".to_string());
    }

    dispatch::persist_page::<source::Ffn>(db, html, page_url, None, listing_locator)
}

/// `fetch_page`'s supersede check: which `FetchTarget` an interrupted fetch
/// belongs to. Pulled free of `AppHandle`/`FetchLock` (the same move already
/// made for `looks_blocked`) so this — previously only exercisable through
/// the full running app — has direct unit test coverage. Prefers the
/// interrupted fetch's own stashed `listing_locator` over decoding its URL
/// when one was stashed (a browse fetch's exact fandom, no decode needed);
/// falls back to `dispatch::resolve_target` otherwise (a chapter fetch, or
/// the debug harness's raw-URL fetch, which never stashes one).
fn resolve_interrupted_target(interrupted: &PendingFetch, db: &db::Db) -> FetchTarget {
    match &interrupted.listing_locator {
        Some(locator) => FetchTarget::Browse { fandom: locator.fandom.clone() },
        None => dispatch::resolve_target::<source::Ffn>(db, &interrupted.url, None),
    }
}

/// Emits one correctly-typed, correctly-targeted error event for a
/// `FetchTarget` — the counterpart to each success arm in
/// `emit_fetch_outcome`. `Unknown` (a URL matching none of the three known
/// shapes — should only happen for a genuinely malformed/unexpected URL, not
/// anything the app itself builds) falls back to the old broadcast-to-all
/// behavior with no identifying fields; every real caller now supplies a
/// `match` predicate keyed on those fields, so an `Unknown` broadcast simply
/// matches none of them rather than misfiring one.
fn emit_targeted_error(app: &tauri::AppHandle, target: FetchTarget, message: String) {
    match target {
        FetchTarget::Chapter { story_id, chapter_num } => {
            let _ = app.emit("chapter-fetch-error", ChapterFetchError { story_id, chapter_num, message });
        }
        FetchTarget::Browse { fandom } => {
            let _ = app.emit("browse-fetch-error", BrowseFetchError { fandom, message });
        }
        FetchTarget::FandomIndex { category } => {
            let _ = app.emit("fandom-index-fetch-error", FandomIndexFetchError { category, message });
        }
        FetchTarget::Unknown => {
            let payload = UnmatchedFetchError { message };
            let _ = app.emit("chapter-fetch-error", payload.clone());
            let _ = app.emit("browse-fetch-error", payload.clone());
            let _ = app.emit("fandom-index-fetch-error", payload);
        }
    }
}

/// Shared outcome-to-events mapping behind both fetch paths
/// (`report_fetch_result`'s window round-trip, `fetch_ao3_page`'s direct
/// HTTP call): same three success events, same error fan-out, differing
/// only in log prefix (`S::LOG_PREFIX`). `page_url`/`fandom_hint` feed
/// `dispatch::resolve_target` on the `Err` arm — see its doc comment.
fn emit_fetch_outcome<S: FicSource>(
    app: &tauri::AppHandle,
    db: &db::Db,
    page_url: &str,
    fandom_hint: Option<&str>,
    result: Result<FetchOutcome, String>,
) {
    let prefix = S::LOG_PREFIX;
    match result {
        Ok(FetchOutcome::Chapter(fetched)) => {
            println!("{prefix} persisted story id={} chapter {}", fetched.story_id, fetched.chapter_num);
            let _ = app.emit("chapter-fetched", fetched);
        }
        Ok(FetchOutcome::Browse(fetched)) => {
            let _ = app.emit("browse-fetched", fetched);
        }
        Ok(FetchOutcome::FandomIndex(fetched)) => {
            let _ = app.emit("fandom-index-fetched", fetched);
        }
        Err(e) => {
            eprintln!("{prefix} failed: {e}");
            let target = dispatch::resolve_target::<S>(db, page_url, fandom_hint);
            emit_targeted_error(app, target, e);
        }
    }
}

#[tauri::command]
pub async fn report_fetch_result(app: tauri::AppHandle, db: tauri::State<'_, db::Db>, html: String, page_url: String, title: String) -> Result<(), ()> {
    // The webview window is left open (still hidden) rather than closed on
    // success — reused via `navigate()` by the next `fetch_page` call (see
    // its doc comment); closing here used to force every fetch back through
    // `WebviewWindowBuilder::build()`, which was what stole OS focus on
    // every chapter of a bulk download.
    let lock = app.state::<FetchLock>();

    // Only trust the stashed locator if it's still *this* fetch's — a newer
    // `fetch_page` call may have already superseded it (see the supersede
    // check in `fetch_page`), in which case treat this fetch as if nothing
    // was stashed at all, same as the debug harness's raw-URL fetch.
    let listing_locator = {
        let pending = lock.0.lock().await;
        pending.as_ref().filter(|p| p.url == page_url).and_then(|p| p.listing_locator.clone())
    };

    let result = try_persist_ffn_fetch(&app, &db, &html, &page_url, &title, listing_locator);

    // Only clear the pending slot if it's still *this* fetch — if a newer
    // `fetch_page` call already superseded it (and already emitted this
    // fetch's error via the check in `fetch_page`), don't clobber the new
    // one's URL.
    let mut pending = lock.0.lock().await;
    if pending.as_ref().map(|p| p.url.as_str()) == Some(page_url.as_str()) {
        *pending = None;
    }
    drop(pending);

    emit_fetch_outcome::<source::Ffn>(&app, &db, &page_url, None, result);
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

/// AO3 counterpart to `fetch_page` — see the module note above on why this
/// doesn't need the webview/event-callback machinery. Emits the same
/// `chapter-fetched`/`browse-fetched`/`*-error` events `report_fetch_result`
/// does, so a future UI (SPEC.md §8, still unwired) can listen the same way
/// regardless of which source triggered the fetch. `fandom_override`, when
/// given, force-applies that label to every row of a tag-listing fetch
/// (ignored for a work/chapter fetch) — see `source::Ao3::apply_fandom_override`.
#[tauri::command]
pub async fn fetch_ao3_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    url: String,
    fandom_override: Option<String>,
) -> Result<(), String> {
    // Classification target: the post-redirect URL once known (AO3 redirects
    // a bare `/works/{id}` to its first chapter), else the originally
    // requested one — a pre-redirect network failure never learns final_url.
    let mut classify_url = url.clone();
    let result = match fetch_ao3_html(&url).await {
        Ok((html, final_url)) => {
            // Dumped unconditionally, same posture as the FFN path's own
            // debug-last-fetch.html — a failed parse should still be inspectable.
            let dump_path = app.path().app_data_dir().expect("resolve app data dir").join("debug-last-ao3-fetch.html");
            let _ = std::fs::write(&dump_path, &html);
            classify_url = final_url.clone();
            dispatch::persist_page::<source::Ao3>(&db, &html, &final_url, fandom_override.as_deref(), None)
        }
        Err(e) => Err(e),
    };
    emit_fetch_outcome::<source::Ao3>(&app, &db, &classify_url, fandom_override.as_deref(), result);
    Ok(())
}

/// Adds one story by pasted URL (`StoryUrlSource`, `dispatch.rs`, validates it
/// and picks the source). Routes to the same fetch path a chapter fetch
/// already uses — `fetch_page` (FFN webview) or `fetch_ao3_page` (direct
/// HTTP) — so the result arrives as the usual `chapter-fetched`/
/// `chapter-fetch-error` event, not through this call's own return. A bad
/// URL fails here, before any fetch starts.
#[tauri::command]
pub async fn add_story_by_url(app: tauri::AppHandle, db: tauri::State<'_, db::Db>, url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    match dispatch::story_url_source(&url)? {
        dispatch::StoryUrlSource::Ffn => fetch_page(app, db, url, None).await,
        dispatch::StoryUrlSource::Ao3 => fetch_ao3_page(app, db, url, None).await,
    }
}

/// Fetches one chapter of an already-imported story — the source, URL and (AO3)
/// chapter id all come from the DB (`dispatch::chapter_fetch_url`), so the
/// frontend sends only `(story_id, chapter_num)`. Always fetches (no cache
/// check — callers ask the DB first). Result arrives as the usual
/// `chapter-fetched`/`chapter-fetch-error` event; an unknown story fails here,
/// before any fetch starts.
#[tauri::command]
pub async fn fetch_chapter(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    story_id: i64,
    chapter_num: i64,
) -> Result<(), String> {
    let (source, url) = dispatch::chapter_fetch_url(&db, story_id, chapter_num)?;
    match source {
        dispatch::StoryUrlSource::Ffn => fetch_page(app, db, url, None).await,
        dispatch::StoryUrlSource::Ao3 => fetch_ao3_page(app, db, url, None).await,
    }
}

/// FFN discovery (SPEC.md §6): fetches the next page of a browse/category
/// search. `category` is required for a brand-new fandom (the "Search FFN"
/// modal always has it); omit it for "Load more" on an already-known
/// fandom, where it's looked up from a prior search instead. Fires the same
/// `browse-fetched`/`browse-fetch-error` events as any other fetch — this
/// only opens the window; `report_fetch_result` does the actual work once
/// the page loads.
#[tauri::command]
pub async fn search_ffn(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    category: Option<String>,
    fandom: String,
    min_word_count: i64,
    status: String,
) -> Result<(), String> {
    let (resolved_category, page) =
        db.run(|conn| db::next_browse_page(conn, category.as_deref(), &fandom, min_word_count, &status))?;
    let url = ffn::build_browse_url(&resolved_category, &fandom, min_word_count, &status, page);
    // Stashed as-requested rather than left to be re-derived by decoding the
    // fetched URL later — see `PendingFetch`'s doc comment.
    let locator = source::FfnListingLocator { category: resolved_category, fandom, min_word_count, status, page };
    fetch_page(app, db, url, Some(locator)).await
}

/// AO3 counterpart to `search_ffn` — no category to resolve (AO3 has none),
/// and no separate window-open step: `fetch_ao3_page` fetches and persists
/// in one call (§8). `sort_column` is hardcoded to `bookmarks_count` — the
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
    status: String,
) -> Result<(), String> {
    let page = db.run(|conn| db::next_ao3_browse_page(conn, &tag, min_word_count, &status))?;
    let url = ao3::build_tag_browse_url(&tag, "bookmarks_count", min_word_count, &status, page);
    fetch_ao3_page(app, db, url, Some(fandom)).await
}

/// Fetches/refreshes a category's fandom list (SPEC.md §6 — precise
/// picker instead of free-text fandom entry). One page covers the whole
/// category (FFN doesn't paginate the index), so this always replaces the
/// cached list wholesale rather than needing "load more" semantics.
#[tauri::command]
pub async fn index_ffn_category(app: tauri::AppHandle, db: tauri::State<'_, db::Db>, category: String) -> Result<(), String> {
    fetch_page(app, db, format!("https://www.fanfiction.net/{category}/"), None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    const FFN_STORY_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_story_page.html"));

    fn test_db() -> db::Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        db::Db(std::sync::Mutex::new(conn))
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

    #[test]
    fn resolve_interrupted_target_prefers_the_stashed_locator_over_decoding_the_url() {
        let dbh = test_db();
        // Same lossy-status URL shape as candidate 1's fix — decoding it
        // alone can't recover "in_progress", but the stashed locator's own
        // fandom is right there regardless of status.
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, "in_progress", 1);
        let interrupted = PendingFetch {
            url,
            listing_locator: Some(source::FfnListingLocator {
                category: "anime".to_string(),
                fandom: "Fullmetal Alchemist".to_string(),
                min_word_count: 0,
                status: "in_progress".to_string(),
                page: 1,
            }),
        };

        match resolve_interrupted_target(&interrupted, &dbh) {
            FetchTarget::Browse { fandom } => assert_eq!(fandom, "Fullmetal Alchemist"),
            _ => panic!("expected FetchTarget::Browse"),
        }
    }

    #[test]
    fn resolve_interrupted_target_falls_back_to_decoding_when_nothing_was_stashed() {
        let dbh = test_db();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";
        dispatch::persist_page::<source::Ffn>(&dbh, FFN_STORY_PAGE, url, None, None).unwrap();
        let interrupted = PendingFetch { url: url.to_string(), listing_locator: None };

        match resolve_interrupted_target(&interrupted, &dbh) {
            FetchTarget::Chapter { chapter_num, .. } => assert_eq!(chapter_num, 34),
            _ => panic!("expected FetchTarget::Chapter"),
        }
    }
}
