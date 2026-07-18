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
                db::upsert_chapter_stub(&conn, story_id, c.chapter_num, &c.title).map_err(|e| e.to_string())?;
            }
            db::upsert_chapter_content(&conn, story_id, chapter_num, chapter_title, &content).map_err(|e| e.to_string())?;

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
            list_fandom_index
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
