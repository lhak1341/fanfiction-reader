//! App-facing read/update operations (SPEC.md §6) — list/filter stories for
//! Search and Bookmarks, toggle the three read_state flags, and serve
//! chapters to the Reader. Built on the persistence primitives in `db.rs`.

use rusqlite::Connection;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryRow {
    pub id: i64,
    pub ffn_story_id: i64,
    pub title: String,
    pub author: String,
    pub fandom: String,
    pub rating: String,
    pub genre: Option<String>,
    pub language: Option<String>,
    pub tags: Option<String>,
    pub status: String,
    pub word_count: i64,
    pub favorite_count: i64,
    pub chapter_count: i64,
    pub summary: Option<String>,
    pub is_read: bool,
    pub dismissed: bool,
    pub bookmarked: bool,
    pub last_chapter_num: Option<i64>,
    /// Raw "Updated:" text from FFN's metadata line, verbatim — see
    /// `StoryMetadata::updated_at`. `None` for one-shots or stories
    /// imported before this field existed.
    pub ffn_updated_at: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryFilters {
    /// `None` or `"All fandoms"` means no fandom filter.
    pub fandom: Option<String>,
    /// Matched against title, author, and summary — never fandom (locked in
    /// the UI/UX ticket: fandom has its own picker, separate from search).
    pub search: Option<String>,
    pub min_word_count: Option<i64>,
    /// `"complete"` | `"in_progress"` | `None` for either.
    pub status: Option<String>,
    /// `"favorites"` (default) | `"updated"` | `"words"`.
    #[serde(default)]
    pub sort_by: String,
    #[serde(default)]
    pub hide_read_and_dismissed: bool,
    /// Bookmarks screen sets this; Search never does.
    #[serde(default)]
    pub bookmarked_only: bool,
}

fn row_to_story(row: &rusqlite::Row) -> rusqlite::Result<StoryRow> {
    Ok(StoryRow {
        id: row.get(0)?,
        ffn_story_id: row.get(1)?,
        title: row.get(2)?,
        author: row.get(3)?,
        fandom: row.get(4)?,
        rating: row.get(5)?,
        genre: row.get(6)?,
        language: row.get(7)?,
        tags: row.get(8)?,
        status: row.get(9)?,
        word_count: row.get(10)?,
        favorite_count: row.get(11)?,
        chapter_count: row.get(12)?,
        summary: row.get(13)?,
        is_read: row.get::<_, i64>(14)? != 0,
        dismissed: row.get::<_, i64>(15)? != 0,
        bookmarked: row.get::<_, i64>(16)? != 0,
        last_chapter_num: row.get(17)?,
        ffn_updated_at: row.get(18)?,
    })
}

pub fn list_stories(conn: &Connection, filters: &StoryFilters) -> rusqlite::Result<Vec<StoryRow>> {
    let mut sql = String::from(
        "SELECT s.id, s.ffn_story_id, s.title, s.author, s.fandom, s.rating, s.genre, s.language, s.tags,
                s.status, s.word_count, s.favorite_count, s.chapter_count, s.summary,
                COALESCE(rs.is_read, 0), COALESCE(rs.dismissed, 0), COALESCE(rs.bookmarked, 0),
                rs.last_chapter_num, s.ffn_updated_at
         FROM stories s
         LEFT JOIN read_state rs ON rs.story_id = s.id
         WHERE 1 = 1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(fandom) = filters.fandom.as_deref().filter(|f| !f.is_empty() && *f != "All fandoms") {
        sql.push_str(" AND s.fandom = ?");
        params.push(Box::new(fandom.to_string()));
    }
    // Text search is folded (accent/case-insensitive, e.g. "poke" matches
    // "Poké") in Rust below rather than pushed into SQL — SQLite's LIKE has
    // no accent-folding without an ICU extension, and the dataset is a
    // single user's personal library (hundreds, not millions, of rows), so
    // filtering the already-narrowed result set in memory is cheap.
    if let Some(min_words) = filters.min_word_count {
        sql.push_str(" AND s.word_count >= ?");
        params.push(Box::new(min_words));
    }
    if let Some(status) = filters.status.as_deref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND s.status = ?");
        params.push(Box::new(status.to_string()));
    }
    if filters.hide_read_and_dismissed {
        sql.push_str(" AND COALESCE(rs.is_read, 0) = 0 AND COALESCE(rs.dismissed, 0) = 0");
    }
    if filters.bookmarked_only {
        sql.push_str(" AND COALESCE(rs.bookmarked, 0) = 1");
    }
    sql.push_str(match filters.sort_by.as_str() {
        "updated" => " ORDER BY s.last_fetched_at DESC",
        "words" => " ORDER BY s.word_count DESC",
        _ => " ORDER BY s.favorite_count DESC",
    });

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        row_to_story,
    )?;
    let mut stories: Vec<StoryRow> = rows.collect::<rusqlite::Result<_>>()?;

    if let Some(search) = filters.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let needle = fold(search);
        stories.retain(|s| {
            fold(&s.title).contains(&needle)
                || fold(&s.author).contains(&needle)
                || s.summary.as_deref().is_some_and(|sm| fold(sm).contains(&needle))
        });
    }

    Ok(stories)
}

/// Lowercases and strips common Latin diacritics so search is
/// accent-insensitive (e.g. "poke" matches "Poké"). Not a full Unicode
/// normalization (no NFD decomposition) — a direct char table covering the
/// accented Latin letters FFN content actually uses, which avoids pulling
/// in a dependency for it.
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ŭ' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

/// Single story by internal id — the Reader needs this (title, fandom,
/// `ffn_story_id` for constructing chapter URLs, current read/bookmark
/// state) but doesn't want to run a filtered list query for it.
pub fn get_story(conn: &Connection, story_id: i64) -> rusqlite::Result<Option<StoryRow>> {
    conn.query_row(
        "SELECT s.id, s.ffn_story_id, s.title, s.author, s.fandom, s.rating, s.genre, s.language, s.tags,
                s.status, s.word_count, s.favorite_count, s.chapter_count, s.summary,
                COALESCE(rs.is_read, 0), COALESCE(rs.dismissed, 0), COALESCE(rs.bookmarked, 0),
                rs.last_chapter_num, s.ffn_updated_at
         FROM stories s
         LEFT JOIN read_state rs ON rs.story_id = s.id
         WHERE s.id = ?1",
        rusqlite::params![story_id],
        row_to_story,
    )
    .map(Some)
    .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })
}

pub fn list_fandoms(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    // Excludes '' — a handful of story-page fetches failed to find the
    // #pre_story_links breadcrumb (fandom extraction fell back to empty)
    // and would otherwise show up as a blank, unlabeled row in the picker.
    let mut stmt = conn.prepare("SELECT DISTINCT fandom FROM stories WHERE fandom != '' ORDER BY fandom")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

/// Scoped to fandoms actually present among bookmarked stories — the
/// Bookmarks screen's fandom picker is deliberately narrower than Search's
/// (locked during UI/UX review).
pub fn list_bookmarked_fandoms(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT s.fandom FROM stories s
         JOIN read_state rs ON rs.story_id = s.id
         WHERE rs.bookmarked = 1 AND s.fandom != ''
         ORDER BY s.fandom",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

fn set_flag(conn: &Connection, story_id: i64, column: &str, timestamp_column: &str, value: bool) -> rusqlite::Result<()> {
    let sql = format!(
        "UPDATE read_state SET {column} = ?1, {timestamp_column} = CASE WHEN ?1 = 1 THEN datetime('now') ELSE NULL END WHERE story_id = ?2"
    );
    conn.execute(&sql, rusqlite::params![value as i64, story_id])?;
    Ok(())
}

pub fn set_read(conn: &Connection, story_id: i64, value: bool) -> rusqlite::Result<()> {
    set_flag(conn, story_id, "is_read", "marked_at", value)
}

pub fn set_dismissed(conn: &Connection, story_id: i64, value: bool) -> rusqlite::Result<()> {
    set_flag(conn, story_id, "dismissed", "dismissed_at", value)
}

pub fn set_bookmarked(conn: &Connection, story_id: i64, value: bool) -> rusqlite::Result<()> {
    set_flag(conn, story_id, "bookmarked", "bookmarked_at", value)
}

/// Called whenever the Reader lands on a chapter (opening or navigating).
/// Also stamps `last_opened_at`, independent of `is_read` — that's what lets
/// [`get_continue_reading`] answer "what was I reading?" without requiring
/// the user to have finished (and marked read) anything.
pub fn set_resume_chapter(conn: &Connection, story_id: i64, chapter_num: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE read_state SET last_chapter_num = ?1, last_opened_at = datetime('now') WHERE story_id = ?2",
        rusqlite::params![chapter_num, story_id],
    )?;
    Ok(())
}

/// Stories with unfinished, in-progress reading — most recently opened
/// first. Feeds a "Continue Reading" surface on the Bookmarks screen.
/// Deliberately excludes `is_read`/`dismissed` stories (nothing to resume)
/// and isn't limited to bookmarked ones (you can be mid-story without
/// having bookmarked it).
pub fn get_continue_reading(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<StoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.ffn_story_id, s.title, s.author, s.fandom, s.rating, s.genre, s.language, s.tags,
                s.status, s.word_count, s.favorite_count, s.chapter_count, s.summary,
                COALESCE(rs.is_read, 0), COALESCE(rs.dismissed, 0), COALESCE(rs.bookmarked, 0),
                rs.last_chapter_num, s.ffn_updated_at
         FROM stories s
         JOIN read_state rs ON rs.story_id = s.id
         WHERE rs.last_opened_at IS NOT NULL AND rs.is_read = 0 AND rs.dismissed = 0
         ORDER BY rs.last_opened_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], row_to_story)?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterListItem {
    pub chapter_num: i64,
    pub title: String,
    pub is_cached: bool,
}

/// Full TOC for a story, including chapters whose content hasn't been
/// fetched yet (`is_cached: false`) — the Reader's chapter drawer needs the
/// whole list to be browsable even before every chapter has been opened.
pub fn list_chapters(conn: &Connection, story_id: i64) -> rusqlite::Result<Vec<ChapterListItem>> {
    let mut stmt = conn.prepare(
        "SELECT chapter_num, title, content <> '' FROM chapters WHERE story_id = ?1 ORDER BY chapter_num",
    )?;
    let rows = stmt.query_map(rusqlite::params![story_id], |row| {
        Ok(ChapterListItem {
            chapter_num: row.get(0)?,
            title: row.get(1)?,
            is_cached: row.get(2)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterContent {
    pub title: String,
    pub content: String,
    pub is_cached: bool,
}

/// `Ok(None)` means this chapter has never been registered at all (the
/// story's TOC hasn't been parsed yet, or the chapter number is out of
/// range) — distinct from `is_cached: false`, which means the TOC knows
/// about it but its page hasn't been fetched.
pub fn get_chapter(conn: &Connection, story_id: i64, chapter_num: i64) -> rusqlite::Result<Option<ChapterContent>> {
    conn.query_row(
        "SELECT title, content, content <> '' FROM chapters WHERE story_id = ?1 AND chapter_num = ?2",
        rusqlite::params![story_id, chapter_num],
        |row| {
            Ok(ChapterContent {
                title: row.get(0)?,
                content: row.get(1)?,
                is_cached: row.get(2)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })
}
