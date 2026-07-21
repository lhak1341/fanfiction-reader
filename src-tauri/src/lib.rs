mod ao3;
mod db;
mod ffn;
mod queries;
mod scrape;
mod source;

use source::{ChapterInfo, FicSource};
use tauri::{Emitter, Manager};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Sanity check for the SQLite schema (SPEC.md §5): confirms every table
/// exists and reports each one's row count.
#[tauri::command]
fn db_health_check(db: tauri::State<db::Db>) -> Result<Vec<String>, String> {
    db.run(|conn| {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
        let table_names: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;

        table_names
            .into_iter()
            .map(|name| {
                let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| row.get(0))?;
                Ok(format!("{name}: {count} row(s)"))
            })
            .collect()
    })
}

/// Serializes access to the single reused `fetch-page` window (see below) —
/// background chapter prefetching can now overlap in time with a foreground
/// navigation fetch, and without this two concurrent `fetch_page` calls can
/// both see the old window, both try to tear it down and rebuild it under
/// the same label, and one loses with "a webview with label `fetch-page`
/// already exists".
pub struct FetchLock(pub tokio::sync::Mutex<()>);

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
async fn fetch_page(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let lock = app.state::<FetchLock>();
    let _guard = lock.0.lock().await;

    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;

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
        .initialization_script(
            r#"
                (function () {
                    var reported = false;
                    function report() {
                        if (reported || !window.__TAURI__) return;
                        reported = true;
                        console.log('[fetch] reporting page (readyState=' + document.readyState + ')');
                        window.__TAURI__.core.invoke('report_fetch_result', {
                            html: document.documentElement.outerHTML,
                            pageUrl: window.location.href,
                            title: document.title
                        }).then(function () {
                            console.log('[fetch] report_fetch_result invoke resolved');
                        }).catch(function (e) {
                            console.error('[fetch] report_fetch_result invoke rejected:', e);
                        });
                    }
                    // DOMContentLoaded fires once HTML is parsed, without
                    // waiting on slow/hanging subresources (ads, trackers,
                    // long-polling) the way `load` or Tauri's own
                    // Finished event do. A timeout fallback covers the rest.
                    document.addEventListener('DOMContentLoaded', report);
                    setTimeout(report, 4000);
                })();
            "#,
        )
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChapterFetched {
    story_id: i64,
    chapter_num: i64,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BrowseFetched {
    fandom: String,
    count: i64,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FandomIndexFetched {
    category: String,
    count: i64,
}

/// `FandomIndex` is FFN-only (no AO3 equivalent, SPEC.md §8) — `fetch_ao3_page`'s
/// dispatch only ever calls `try_persist_fetch::<source::Ao3>`, which structurally
/// never produces it, so that match arm is `unreachable!()` rather than handled.
enum FetchOutcome {
    Chapter(ChapterFetched),
    Browse(BrowseFetched),
    FandomIndex(FandomIndexFetched),
}

/// Generic parse+persist dispatch shared by every `FicSource` (`source.rs`):
/// tries the chapter shape first, then the listing shape, else errors.
/// `report_fetch_result`/`fetch_ao3_page` below turn the `Result` into
/// `chapter-fetched`/`browse-fetched`/`*-error` events, since the command
/// that kicked off the fetch already returned by the time this runs.
fn try_persist_fetch<S: FicSource>(
    db: &db::Db,
    html: &str,
    page_url: &str,
    fandom_override: Option<&str>,
) -> Result<FetchOutcome, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;

    if let Some(locator) = S::chapter_locator_from_url(page_url)? {
        let (meta, chapters) = S::parse_work(html, &locator)?;
        let content = S::parse_chapter_content(html)?;
        let chapter_num = S::resolve_chapter_num(&chapters, &locator)?;
        let matched = chapters.iter().find(|c| c.chapter_num() == chapter_num);
        let chapter_title = matched.map(|c| c.title()).unwrap_or_else(|| S::metadata_title(&meta));
        let source_chapter_id = matched.and_then(|c| c.source_chapter_id());

        println!(
            "[fetch] {} — chapter {chapter_num} ({} bytes), {} chapters in TOC",
            S::metadata_title(&meta),
            content.len(),
            chapters.len()
        );

        let story_id = S::persist_story(&conn, &meta).map_err(|e| e.to_string())?;
        db::ensure_read_state(&conn, story_id).map_err(|e| e.to_string())?;
        // Register the *full* TOC every time — any story page lists every
        // chapter, so this keeps titles browsable even before their own
        // pages have been fetched. Only the chapter we actually
        // navigated to gets content.
        for c in &chapters {
            db::upsert_chapter_stub(&conn, story_id, c.chapter_num(), c.title(), c.source_chapter_id())
                .map_err(|e| e.to_string())?;
        }
        db::upsert_chapter_content(&conn, story_id, chapter_num, chapter_title, &content, source_chapter_id)
            .map_err(|e| e.to_string())?;

        return Ok(FetchOutcome::Chapter(ChapterFetched { story_id, chapter_num }));
    }

    if let Some(locator) = S::listing_locator_from_url(page_url) {
        let mut summaries = S::parse_listing(html, &locator)?;
        if summaries.is_empty() {
            return Err("no stories found on this page — not a listing, or the layout changed".to_string());
        }
        if let Some(label) = fandom_override {
            for meta in &mut summaries {
                S::apply_fandom_override(meta, label);
            }
        }
        for meta in &summaries {
            let story_id = S::persist_story(&conn, meta).map_err(|e| e.to_string())?;
            db::ensure_read_state(&conn, story_id).map_err(|e| e.to_string())?;
        }
        // Round-tripped from the same URL params the search command wrote,
        // so record_listing_page's key matches what "Load more" will look
        // up next time this exact combo is searched.
        let page = S::listing_page(&locator);
        S::record_listing_page(&conn, &locator, page).map_err(|e| e.to_string())?;

        let fandom = S::metadata_fandom(&summaries[0]).to_string();
        println!("[fetch] imported {} stories into fandom {fandom:?}, page {page}", summaries.len());
        return Ok(FetchOutcome::Browse(BrowseFetched { fandom, count: summaries.len() as i64 }));
    }

    Err("could not determine what kind of page this is (chapter, or listing)".to_string())
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

/// FFN-specific pre-checks (debug dump, Cloudflare-block detection) plus the
/// FFN-only category-index shape (no AO3 equivalent, SPEC.md §8), then falls
/// through to the shared `try_persist_fetch::<source::Ffn>` dispatch.
fn try_persist_ffn_fetch(
    app: &tauri::AppHandle,
    db: &tauri::State<db::Db>,
    html: &str,
    page_url: &str,
    title: &str,
) -> Result<FetchOutcome, String> {
    // Dumped to disk unconditionally (before the blocked check) so a false
    // positive there can still be inspected, not just a genuine block.
    let dump_path = app.path().app_data_dir().expect("resolve app data dir").join("debug-last-fetch.html");
    let _ = std::fs::write(&dump_path, html);

    if looks_blocked(html, title) {
        return Err("Cloudflare challenge page, not the story — try again".to_string());
    }

    if let Some(category) = ffn::category_from_index_url(page_url) {
        let mut conn = db.0.lock().map_err(|e| e.to_string())?;
        let entries = ffn::parse_fandom_index(html, 1000);
        if entries.is_empty() {
            return Err("no fandoms found on this page — not a category index, or the layout changed".to_string());
        }
        db::replace_fandom_index(&mut conn, &category, &entries).map_err(|e| e.to_string())?;
        println!("[fetch] indexed {} fandoms (1000+ fics) for category {category}", entries.len());
        return Ok(FetchOutcome::FandomIndex(FandomIndexFetched { category, count: entries.len() as i64 }));
    }

    try_persist_fetch::<source::Ffn>(db, html, page_url, None)
}

#[tauri::command]
fn report_fetch_result(app: tauri::AppHandle, db: tauri::State<db::Db>, html: String, page_url: String, title: String) {
    match try_persist_ffn_fetch(&app, &db, &html, &page_url, &title) {
        Ok(FetchOutcome::Chapter(fetched)) => {
            println!("[fetch] persisted story id={} chapter {}", fetched.story_id, fetched.chapter_num);
            let _ = app.emit("chapter-fetched", fetched);
            // Left open (still hidden) rather than closed on success — it's
            // reused via `navigate()` by the next `fetch_page` call (see its
            // doc comment). Closing here used to force every single fetch
            // back through `WebviewWindowBuilder::build()`, which is what
            // was actually causing the app to steal OS focus on every
            // chapter of a bulk download, not window creation being
            // unavoidable in general.
        }
        Ok(FetchOutcome::Browse(fetched)) => {
            let _ = app.emit("browse-fetched", fetched);
        }
        Ok(FetchOutcome::FandomIndex(fetched)) => {
            let _ = app.emit("fandom-index-fetched", fetched);
        }
        Err(e) => {
            eprintln!("[fetch] failed: {e}");
            let _ = app.emit("chapter-fetch-error", e.clone());
            let _ = app.emit("browse-fetch-error", e.clone());
            let _ = app.emit("fandom-index-fetch-error", e);
        }
    }
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
async fn fetch_ao3_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    url: String,
    fandom_override: Option<String>,
) -> Result<(), String> {
    let result = match fetch_ao3_html(&url).await {
        Ok((html, final_url)) => {
            // Dumped unconditionally, same posture as the FFN path's own
            // debug-last-fetch.html — a failed parse should still be inspectable.
            let dump_path = app.path().app_data_dir().expect("resolve app data dir").join("debug-last-ao3-fetch.html");
            let _ = std::fs::write(&dump_path, &html);
            try_persist_fetch::<source::Ao3>(&db, &html, &final_url, fandom_override.as_deref())
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(FetchOutcome::Chapter(fetched)) => {
            println!("[ao3-fetch] persisted story id={} chapter {}", fetched.story_id, fetched.chapter_num);
            let _ = app.emit("chapter-fetched", fetched);
        }
        Ok(FetchOutcome::Browse(fetched)) => {
            let _ = app.emit("browse-fetched", fetched);
        }
        Ok(FetchOutcome::FandomIndex(_)) => unreachable!("AO3 never produces a FandomIndex outcome"),
        Err(e) => {
            eprintln!("[ao3-fetch] failed: {e}");
            let _ = app.emit("chapter-fetch-error", e.clone());
            let _ = app.emit("browse-fetch-error", e);
        }
    }
    Ok(())
}

// ---- Query/toggle commands (SPEC.md §6) ----

#[tauri::command]
fn list_stories(db: tauri::State<db::Db>, filters: queries::StoryFilters) -> Result<Vec<queries::StoryRow>, String> {
    db.run(|conn| queries::list_stories(conn, &filters))
}

#[tauri::command]
fn list_bookmarks(db: tauri::State<db::Db>, mut filters: queries::StoryFilters) -> Result<Vec<queries::StoryRow>, String> {
    filters.bookmarked_only = true;
    db.run(|conn| queries::list_stories(conn, &filters))
}

#[tauri::command]
fn get_story(db: tauri::State<db::Db>, story_id: i64) -> Result<Option<queries::StoryRow>, String> {
    db.run(|conn| queries::get_story(conn, story_id))
}

#[tauri::command]
fn list_fandoms(db: tauri::State<db::Db>) -> Result<Vec<String>, String> {
    db.run(|conn| queries::list_fandoms(conn))
}

#[tauri::command]
fn rename_fandom(db: tauri::State<db::Db>, old_fandom: String, new_fandom: String) -> Result<usize, String> {
    db.run(|conn| queries::rename_fandom(conn, &old_fandom, &new_fandom))
}

#[tauri::command]
fn list_bookmarked_fandoms(db: tauri::State<db::Db>) -> Result<Vec<String>, String> {
    db.run(|conn| queries::list_bookmarked_fandoms(conn))
}

#[tauri::command]
fn toggle_read(db: tauri::State<db::Db>, story_id: i64, value: bool) -> Result<(), String> {
    db.run(|conn| queries::set_read(conn, story_id, value))
}

#[tauri::command]
fn toggle_dismissed(db: tauri::State<db::Db>, story_id: i64, value: bool) -> Result<(), String> {
    db.run(|conn| queries::set_dismissed(conn, story_id, value))
}

#[tauri::command]
fn toggle_bookmarked(db: tauri::State<db::Db>, story_id: i64, value: bool) -> Result<(), String> {
    db.run(|conn| queries::set_bookmarked(conn, story_id, value))
}

#[tauri::command]
fn set_resume_chapter(db: tauri::State<db::Db>, story_id: i64, chapter_num: i64) -> Result<(), String> {
    db.run(|conn| queries::set_resume_chapter(conn, story_id, chapter_num))
}

#[tauri::command]
fn list_chapters(db: tauri::State<db::Db>, story_id: i64) -> Result<Vec<queries::ChapterListItem>, String> {
    db.run(|conn| queries::list_chapters(conn, story_id))
}

#[tauri::command]
fn get_chapter(db: tauri::State<db::Db>, story_id: i64, chapter_num: i64) -> Result<Option<queries::ChapterContent>, String> {
    db.run(|conn| queries::get_chapter(conn, story_id, chapter_num))
}

#[tauri::command]
fn get_continue_reading(db: tauri::State<db::Db>, limit: i64) -> Result<Vec<queries::StoryRow>, String> {
    db.run(|conn| queries::get_continue_reading(conn, limit))
}

/// FFN discovery (SPEC.md §6): fetches the next page of a browse/category
/// search. `category` is required for a brand-new fandom (the "Search FFN"
/// modal always has it); omit it for "Load more" on an already-known
/// fandom, where it's looked up from a prior search instead. Fires the same
/// `browse-fetched`/`browse-fetch-error` events as any other fetch — this
/// only opens the window; `report_fetch_result` does the actual work once
/// the page loads.
#[tauri::command]
async fn search_ffn(
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
    fetch_page(app, url).await
}

/// AO3 counterpart to `search_ffn` — no category to resolve (AO3 has none),
/// and no separate window-open step: `fetch_ao3_page` fetches and persists
/// in one call (§8). `sort_column` is hardcoded to `bookmarks_count` — the
/// closer analog to FFN's "always sort by Favorites for import relevance"
/// (§8 locked: AO3's bookmarks, not kudos, is the comparable deliberate
/// save/track signal to FFN's Favorites). `fandom` is force-applied to every
/// imported row (see `try_persist_fetch`'s `fandom_override` param) so it
/// lands in the same fandom bucket as however this fandom is already named
/// on the FFN side — the whole point of searching both sources from one
/// Search screen, not two separate fandom lists (user-confirmed 2026-07-19).
#[tauri::command]
async fn search_ao3(
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
async fn index_ffn_category(app: tauri::AppHandle, category: String) -> Result<(), String> {
    fetch_page(app, format!("https://www.fanfiction.net/{category}/")).await
}

#[tauri::command]
fn list_fandom_index(db: tauri::State<db::Db>, category: Option<String>) -> Result<Vec<db::FandomIndexEntry>, String> {
    db.run(|conn| db::list_fandom_index(conn, category.as_deref()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let conn = db::init(&app.handle())?;
            app.manage(db::Db(std::sync::Mutex::new(conn)));
            app.manage(FetchLock(tokio::sync::Mutex::new(())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            fetch_page,
            report_fetch_result,
            db_health_check,
            list_stories,
            list_bookmarks,
            get_story,
            list_fandoms,
            rename_fandom,
            list_bookmarked_fandoms,
            toggle_read,
            toggle_dismissed,
            toggle_bookmarked,
            set_resume_chapter,
            list_chapters,
            get_chapter,
            get_continue_reading,
            search_ffn,
            index_ffn_category,
            list_fandom_index,
            fetch_ao3_page,
            search_ao3
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    const FFN_STORY_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_story_page.html"));
    const FFN_BROWSE_LISTING: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_browse_listing.html"));
    const AO3_WORK_PAGE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ao3_work_page.html"));
    const AO3_LISTING_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ao3_listing_page.html"));

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
    fn ffn_chapter_url_dispatches_to_chapter_branch_and_resolves_chapter_num() {
        let dbh = test_db();
        let url = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

        let outcome = try_persist_fetch::<source::Ffn>(&dbh, FFN_STORY_PAGE, url, None).unwrap();

        match outcome {
            FetchOutcome::Chapter(fetched) => assert_eq!(fetched.chapter_num, 34),
            _ => panic!("expected a Chapter outcome for a chapter URL"),
        }
    }

    #[test]
    fn ffn_listing_url_dispatches_to_browse_branch() {
        let dbh = test_db();
        let url = ffn::build_browse_url("anime", "Fullmetal Alchemist", 0, "any", 1);

        let outcome = try_persist_fetch::<source::Ffn>(&dbh, FFN_BROWSE_LISTING, &url, None).unwrap();

        match outcome {
            FetchOutcome::Browse(fetched) => assert_eq!(fetched.count, 25),
            _ => panic!("expected a Browse outcome for a listing URL"),
        }
    }

    #[test]
    fn empty_listing_page_is_an_error_not_a_zero_count_browse_outcome() {
        let dbh = test_db();
        let url = ffn::build_browse_url("anime", "Nonexistent Fandom", 0, "any", 1);

        // FFN_STORY_PAGE is a chapter page, not a listing — parse_browse_page
        // against it should find zero story rows.
        let result = try_persist_fetch::<source::Ffn>(&dbh, FFN_STORY_PAGE, &url, None);

        assert!(result.is_err(), "expected an error for a listing page with zero story rows");
    }

    #[test]
    fn ao3_fandom_override_applies_on_listing_rows() {
        let dbh = test_db();
        let url = ao3::build_tag_browse_url("Harry Potter", "bookmarks_count", 0, "any", 1);

        try_persist_fetch::<source::Ao3>(&dbh, AO3_LISTING_PAGE, &url, Some("Overridden Fandom")).unwrap();

        let conn = dbh.0.lock().unwrap();
        let fandom: String = conn.query_row("SELECT fandom FROM stories LIMIT 1", [], |row| row.get(0)).unwrap();
        assert_eq!(fandom, "Overridden Fandom");
    }

    #[test]
    fn ao3_fandom_override_is_ignored_on_the_chapter_path() {
        let dbh = test_db();
        let url = "https://archiveofourown.org/works/85954646/chapters/227238471";

        let outcome =
            try_persist_fetch::<source::Ao3>(&dbh, AO3_WORK_PAGE, url, Some("Should Not Apply Here")).unwrap();

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
}
