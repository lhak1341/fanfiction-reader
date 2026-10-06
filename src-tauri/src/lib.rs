mod ao3;
mod db;
mod dispatch;
mod fetch;
mod ffn;
mod migrations;
mod queries;
mod scrape;
mod source;

use fetch::{add_story_by_url, fetch_ao3_page, fetch_page, index_ffn_category, report_fetch_result, search_ao3, search_ffn, FetchLock};
use tauri::Manager;

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
            app.manage(FetchLock(tokio::sync::Mutex::new(None)));
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
            search_ao3,
            add_story_by_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
