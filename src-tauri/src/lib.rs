mod ao3;
mod db;
mod ffn;
mod queries;

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
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(|e| e.to_string())?;
    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;

    table_names
        .into_iter()
        .map(|name| {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            Ok(format!("{name}: {count} row(s)"))
        })
        .collect()
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
#[tauri::command]
async fn fetch_page(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let lock = app.state::<FetchLock>();
    let _guard = lock.0.lock().await;

    // destroy(), not close(): close() emits a CloseRequested event and goes
    // through the normal (interceptable, not-necessarily-immediate) close
    // pipeline — the next line's WebviewWindowBuilder::build() could run
    // before the label is actually freed. destroy() force-closes with no
    // event round-trip, then we poll (briefly) to confirm the label is
    // clear before reusing it.
    if let Some(existing) = app.get_webview_window("fetch-page") {
        let _ = existing.destroy();
        for _ in 0..50 {
            if app.get_webview_window("fetch-page").is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;

    println!("[fetch] opening window for {parsed}");

    // Tauri's own on_page_load(Finished) turned out unreliable on some FFN
    // pages (browse/category pages in particular) — never fired even though
    // the page visibly rendered correctly, likely confused by a subframe
    // (ad/tracking iframe) navigating independently. Triggering off the
    // page's own native `load` event instead, via an init script that runs
    // before the page's own scripts on every navigation, tracks what's
    // actually on screen rather than Tauri's navigation-delegate state.
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

enum FetchOutcome {
    Chapter(ChapterFetched),
    Browse(BrowseFetched),
    FandomIndex(FandomIndexFetched),
}

/// Does the actual parse+persist work; `report_fetch_result` below turns its
/// `Result` into `chapter-fetched`/`browse-fetched`/`*-error` events, since
/// the command that opened the window (`fetch_page`) already returned by the
/// time this fires — callers can't just `await` a return value here.
///
/// Branches on the URL shape: `/s/{id}/{num}/...` is a chapter page: fetch
/// its content, persist it, register the story's TOC. Anything else is
/// treated as a browse/category listing page (SPEC.md §6 discovery loop):
/// parse every story summary on it and upsert each as a stub (metadata
/// only — chapters get fetched lazily like any other story once opened).
fn try_persist_fetch(
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

    // Scoped to the <title> (never full page text) — story prose is free-form
    // English and can innocently contain phrases like "just a moment"; a
    // Cloudflare block page's *title* is one of a small fixed set that a real
    // FFN chapter title (always "<story> Chapter N: ... | FanFiction") can't
    // plausibly collide with. `cf-browser-verification` is a real CSS class
    // Cloudflare's interstitial injects, safe to check against the full page.
    let title_lower = title.to_lowercase();
    let looks_blocked = title_lower.contains("just a moment")
        || title_lower.contains("attention required")
        || title_lower.contains("checking your browser")
        || html.contains("cf-browser-verification");
    if looks_blocked {
        return Err("Cloudflare challenge page, not the story — try again".to_string());
    }

    let mut conn = db.0.lock().map_err(|e| e.to_string())?;

    match (ffn::story_id_from_url(page_url), ffn::chapter_num_from_url(page_url)) {
        (Ok(ffn_story_id), Ok(chapter_num)) => {
            let (meta, chapters) = ffn::parse_story_page(html, ffn_story_id)?;
            let content = ffn::parse_chapter_content(html)?;

            println!(
                "[fetch] {} — chapter {chapter_num} ({} bytes), {} chapters in TOC",
                meta.title,
                content.len(),
                chapters.len()
            );

            let chapter_title = chapters
                .iter()
                .find(|c| c.chapter_num == chapter_num)
                .map(|c| c.title.as_str())
                .unwrap_or(&meta.title);

            let story_id = db::upsert_story(&conn, &meta).map_err(|e| e.to_string())?;
            db::ensure_read_state(&conn, story_id).map_err(|e| e.to_string())?;
            // Register the *full* TOC every time — any story page lists every
            // chapter, so this keeps titles browsable even before their own
            // pages have been fetched. Only the chapter we actually
            // navigated to gets content.
            for c in &chapters {
                db::upsert_chapter_stub(&conn, story_id, c.chapter_num, &c.title, None).map_err(|e| e.to_string())?;
            }
            db::upsert_chapter_content(&conn, story_id, chapter_num, chapter_title, &content, None).map_err(|e| e.to_string())?;

            Ok(FetchOutcome::Chapter(ChapterFetched { story_id, chapter_num }))
        }
        _ if ffn::parse_browse_url(page_url).is_some() => {
            let (category, fandom, page) = ffn::parse_browse_url(page_url).unwrap();
            let summaries = ffn::parse_browse_page(html, &fandom)?;
            if summaries.is_empty() {
                return Err("no stories found on this page — not a browse listing, or the layout changed".to_string());
            }
            for meta in &summaries {
                let story_id = db::upsert_story(&conn, meta).map_err(|e| e.to_string())?;
                db::ensure_read_state(&conn, story_id).map_err(|e| e.to_string())?;
            }
            // Round-tripped from the same URL params build_browse_url wrote,
            // so record_browse_page's key matches what next_browse_page will
            // look up next time "Load more" runs for this exact combo.
            let parsed_url = url::Url::parse(page_url).map_err(|e| e.to_string())?;
            let min_word_count: i64 = parsed_url
                .query_pairs()
                .find(|(k, _)| k == "len")
                .and_then(|(_, v)| v.parse::<i64>().ok())
                .map(|thousands| thousands * 1000)
                .unwrap_or(0);
            let status = if parsed_url.query_pairs().any(|(k, v)| k == "s" && v == "2") { "complete" } else { "any" };
            db::record_browse_page(&conn, &category, &fandom, min_word_count, status, page).map_err(|e| e.to_string())?;

            println!("[fetch] imported {} stories from {fandom} browse page {page}", summaries.len());
            Ok(FetchOutcome::Browse(BrowseFetched { fandom, count: summaries.len() as i64 }))
        }
        _ if ffn::category_from_index_url(page_url).is_some() => {
            let category = ffn::category_from_index_url(page_url).unwrap();
            let entries = ffn::parse_fandom_index(html, 1000);
            if entries.is_empty() {
                return Err("no fandoms found on this page — not a category index, or the layout changed".to_string());
            }
            db::replace_fandom_index(&mut conn, &category, &entries).map_err(|e| e.to_string())?;
            println!("[fetch] indexed {} fandoms (1000+ fics) for category {category}", entries.len());
            Ok(FetchOutcome::FandomIndex(FandomIndexFetched { category, count: entries.len() as i64 }))
        }
        _ => Err("could not determine what kind of FFN page this is (chapter, browse listing, or category index)".to_string()),
    }
}

#[tauri::command]
fn report_fetch_result(app: tauri::AppHandle, db: tauri::State<db::Db>, html: String, page_url: String, title: String) {
    match try_persist_fetch(&app, &db, &html, &page_url, &title) {
        Ok(FetchOutcome::Chapter(fetched)) => {
            println!("[fetch] persisted story id={} chapter {}", fetched.story_id, fetched.chapter_num);
            let _ = app.emit("chapter-fetched", fetched);
            // Auto-close on success — a Reader-triggered fetch shouldn't leave
            // a window sitting open. Left open on error so the page (a
            // Cloudflare challenge, an FFN error page, etc.) stays visible.
            if let Some(w) = app.get_webview_window("fetch-page") {
                let _ = w.close();
            }
        }
        Ok(FetchOutcome::Browse(fetched)) => {
            let _ = app.emit("browse-fetched", fetched);
            if let Some(w) = app.get_webview_window("fetch-page") {
                let _ = w.close();
            }
        }
        Ok(FetchOutcome::FandomIndex(fetched)) => {
            let _ = app.emit("fandom-index-fetched", fetched);
            if let Some(w) = app.get_webview_window("fetch-page") {
                let _ = w.close();
            }
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

/// Same shape as `try_persist_fetch` (parse, upsert, register TOC) but for
/// AO3's markup/schema (`ao3.rs`, `db::upsert_ao3_story`). Dispatches on
/// `final_url` (post-redirect — see `fetch_ao3_html`), not the URL that was
/// originally requested.
fn try_persist_ao3_fetch(
    app: &tauri::AppHandle,
    db: &tauri::State<db::Db>,
    html: &str,
    final_url: &str,
    fandom_override: Option<&str>,
) -> Result<FetchOutcome, String> {
    let dump_path = app.path().app_data_dir().expect("resolve app data dir").join("debug-last-ao3-fetch.html");
    let _ = std::fs::write(&dump_path, html);

    let conn = db.0.lock().map_err(|e| e.to_string())?;

    if let Ok(work_id) = ao3::work_id_from_url(final_url) {
        let (meta, chapters) = ao3::parse_work_page(html, work_id)?;
        let content = ao3::parse_chapter_content(html)?;
        let chapter_id = ao3::chapter_id_from_url(final_url)?;

        let chapter_num = match chapter_id {
            Some(id) => chapters
                .iter()
                .find(|c| c.chapter_id == Some(id))
                .map(|c| c.chapter_num)
                .ok_or("fetched chapter id not found in this work's own TOC")?,
            // No /chapters/{id} in the URL: only valid for a one-shot,
            // which parse_work_page represents as a single synthetic
            // chapter with chapter_id: None (see Ao3ChapterRef::chapter_id).
            None if chapters.len() == 1 => 1,
            None => return Err("multi-chapter work URL had no chapter id — expected a redirect to one".to_string()),
        };

        println!(
            "[ao3-fetch] {} — chapter {chapter_num} ({} bytes), {} chapters in TOC",
            meta.title,
            content.len(),
            chapters.len()
        );

        let chapter_title = chapters
            .iter()
            .find(|c| c.chapter_num == chapter_num)
            .map(|c| c.title.as_str())
            .unwrap_or(&meta.title);

        let story_id = db::upsert_ao3_story(&conn, &meta).map_err(|e| e.to_string())?;
        db::ensure_read_state(&conn, story_id).map_err(|e| e.to_string())?;
        for c in &chapters {
            db::upsert_chapter_stub(&conn, story_id, c.chapter_num, &c.title, c.chapter_id).map_err(|e| e.to_string())?;
        }
        db::upsert_chapter_content(&conn, story_id, chapter_num, chapter_title, &content, chapter_id)
            .map_err(|e| e.to_string())?;

        return Ok(FetchOutcome::Chapter(ChapterFetched { story_id, chapter_num }));
    }

    if let Some(tag) = ao3::tag_from_listing_url(final_url) {
        let mut summaries = ao3::parse_listing_page(html)?;
        if summaries.is_empty() {
            return Err("no works found on this page — not a tag listing, or the layout changed".to_string());
        }
        // Force-apply the caller's chosen fandom label to every row — same
        // thing FFN's own browse import already does (§4: "fandom isn't
        // repeated per row; it's derived once ... and applied to every
        // parsed entry"). Without this, AO3's own canonical fandom text
        // (e.g. "Harry Potter - J. K. Rowling") lands in a separate fandom
        // bucket from whatever the user already calls it on the FFN side,
        // defeating the point of searching both sources from one Search
        // screen (user-confirmed 2026-07-19).
        if let Some(label) = fandom_override {
            for meta in &mut summaries {
                meta.fandom = label.to_string();
            }
        }
        for meta in &summaries {
            let story_id = db::upsert_ao3_story(&conn, meta).map_err(|e| e.to_string())?;
            db::ensure_read_state(&conn, story_id).map_err(|e| e.to_string())?;
        }
        // Round-tripped from the same query params build_tag_browse_url
        // wrote, same as FFN's browse_state bookkeeping (below) — so
        // next_ao3_browse_page's lookup key matches what "Load more" will
        // ask for next time for this exact tag+filter combo.
        let parsed_url = url::Url::parse(final_url).map_err(|e| e.to_string())?;
        let min_word_count: i64 = parsed_url
            .query_pairs()
            .find(|(k, _)| k == "work_search[words_from]")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let status = match parsed_url.query_pairs().find(|(k, _)| k == "work_search[complete]").map(|(_, v)| v.into_owned()) {
            Some(v) if v == "T" => "complete",
            Some(v) if v == "F" => "in_progress",
            _ => "any",
        };
        let page: i64 = parsed_url.query_pairs().find(|(k, _)| k == "page").and_then(|(_, v)| v.parse().ok()).unwrap_or(1);
        db::record_ao3_browse_page(&conn, &tag, min_word_count, status, page).map_err(|e| e.to_string())?;

        // Report back whatever actually ended up in `fandom` (the override,
        // or — if none was given, e.g. a future non-search caller — each
        // row's own parsed AO3 fandom text) so the caller's fandom
        // filter/selection matches what really got stored.
        let fandom = summaries.first().map(|s| s.fandom.clone()).unwrap_or(tag);
        println!("[ao3-fetch] imported {} works into fandom {fandom:?}, page {page}", summaries.len());
        return Ok(FetchOutcome::Browse(BrowseFetched { fandom, count: summaries.len() as i64 }));
    }

    Err("could not determine what kind of AO3 page this is (work/chapter, or tag listing)".to_string())
}

/// AO3 counterpart to `fetch_page` — see the module note above on why this
/// doesn't need the webview/event-callback machinery. Emits the same
/// `chapter-fetched`/`browse-fetched`/`*-error` events `report_fetch_result`
/// does, so a future UI (SPEC.md §8, still unwired) can listen the same way
/// regardless of which source triggered the fetch. `fandom_override`, when
/// given, force-applies that label to every row of a tag-listing fetch
/// (ignored for a work/chapter fetch) — see `try_persist_ao3_fetch`.
#[tauri::command]
async fn fetch_ao3_page(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    url: String,
    fandom_override: Option<String>,
) -> Result<(), String> {
    let result = match fetch_ao3_html(&url).await {
        Ok((html, final_url)) => try_persist_ao3_fetch(&app, &db, &html, &final_url, fandom_override.as_deref()),
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
        Ok(FetchOutcome::FandomIndex(_)) => unreachable!("try_persist_ao3_fetch never returns FandomIndex"),
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
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_stories(&conn, &filters).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_bookmarks(db: tauri::State<db::Db>, mut filters: queries::StoryFilters) -> Result<Vec<queries::StoryRow>, String> {
    filters.bookmarked_only = true;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_stories(&conn, &filters).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_story(db: tauri::State<db::Db>, story_id: i64) -> Result<Option<queries::StoryRow>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_story(&conn, story_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_fandoms(db: tauri::State<db::Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_fandoms(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_fandom(db: tauri::State<db::Db>, old_fandom: String, new_fandom: String) -> Result<usize, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::rename_fandom(&conn, &old_fandom, &new_fandom).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_bookmarked_fandoms(db: tauri::State<db::Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_bookmarked_fandoms(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_read(db: tauri::State<db::Db>, story_id: i64, value: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::set_read(&conn, story_id, value).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_dismissed(db: tauri::State<db::Db>, story_id: i64, value: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::set_dismissed(&conn, story_id, value).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_bookmarked(db: tauri::State<db::Db>, story_id: i64, value: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::set_bookmarked(&conn, story_id, value).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_resume_chapter(db: tauri::State<db::Db>, story_id: i64, chapter_num: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::set_resume_chapter(&conn, story_id, chapter_num).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_chapters(db: tauri::State<db::Db>, story_id: i64) -> Result<Vec<queries::ChapterListItem>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_chapters(&conn, story_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_chapter(db: tauri::State<db::Db>, story_id: i64, chapter_num: i64) -> Result<Option<queries::ChapterContent>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_chapter(&conn, story_id, chapter_num).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_continue_reading(db: tauri::State<db::Db>, limit: i64) -> Result<Vec<queries::StoryRow>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_continue_reading(&conn, limit).map_err(|e| e.to_string())
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
    let (resolved_category, page) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        db::next_browse_page(&conn, category.as_deref(), &fandom, min_word_count, &status)
            .map_err(|e| e.to_string())?
    };
    let url = ffn::build_browse_url(&resolved_category, &fandom, min_word_count, &status, page);
    fetch_page(app, url).await
}

/// AO3 counterpart to `search_ffn` — no category to resolve (AO3 has none),
/// and no separate window-open step: `fetch_ao3_page` fetches and persists
/// in one call (§8). `sort_column` is hardcoded to `bookmarks_count` — the
/// closer analog to FFN's "always sort by Favorites for import relevance"
/// (§8 locked: AO3's bookmarks, not kudos, is the comparable deliberate
/// save/track signal to FFN's Favorites). `fandom` is force-applied to every
/// imported row (see `try_persist_ao3_fetch`'s `fandom_override`) so it
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
    let page = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        db::next_ao3_browse_page(&conn, &tag, min_word_count, &status).map_err(|e| e.to_string())?
    };
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
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::list_fandom_index(&conn, category.as_deref()).map_err(|e| e.to_string())
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
