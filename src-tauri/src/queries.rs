//! App-facing read/update operations (SPEC.md §6) — list/filter stories for
//! Search and Bookmarks, toggle the three read_state flags, and serve
//! chapters to the Reader. Built on the persistence primitives in `db.rs`.

use rusqlite::Connection;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryRow {
    pub id: i64,
    /// Aliased from `stories.source_story_id` (SPEC.md §8) — the FFN work id
    /// for `"ffn"` rows, the AO3 work id for `"ao3"` rows.
    pub source_story_id: i64,
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
    /// `"ffn"` | `"ao3"` — which source this row came from (SPEC.md §8).
    /// The Reader needs this to know which fetch command/URL shape to use
    /// for an uncached chapter.
    pub source: String,
    /// How many of `chapter_count` chapters already have `content <> ''`.
    /// `== chapter_count` means the story is fully cached (offline-ready) —
    /// drives Bookmarks' "downloaded" badge (SPEC.md §6).
    pub cached_chapter_count: i64,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryFilters {
    /// `None` or empty means no fandom filter.
    pub fandom: Option<String>,
    /// `"ffn"` | `"ao3"` | `None` for both — SPEC.md §8's locked "sources
    /// aren't merged into one ranking" means sort-by-favorites only makes
    /// sense scoped to one source at a time; this is that scope control.
    pub source: Option<String>,
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
    pub hide_read: bool,
    #[serde(default)]
    pub hide_dismissed: bool,
    #[serde(default)]
    pub hide_bookmarked: bool,
}

/// Which stories a [`list_stories`] call ranges over. Not part of
/// [`StoryFilters`] (which the frontend deserializes into): the Bookmarks
/// screen's "bookmarked only" rule is the backend's, so a client can't set
/// it by adding a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    BookmarksOnly,
}

fn row_to_story(row: &rusqlite::Row) -> rusqlite::Result<StoryRow> {
    Ok(StoryRow {
        id: row.get("id")?,
        source_story_id: row.get("source_story_id")?,
        title: row.get("title")?,
        author: row.get("author")?,
        fandom: row.get("fandom")?,
        rating: row.get("rating")?,
        genre: row.get("genre")?,
        language: row.get("language")?,
        tags: row.get("tags")?,
        status: row.get("status")?,
        word_count: row.get("word_count")?,
        favorite_count: row.get("favorite_count")?,
        chapter_count: row.get("chapter_count")?,
        summary: row.get("summary")?,
        is_read: row.get::<_, i64>("is_read")? != 0,
        dismissed: row.get::<_, i64>("dismissed")? != 0,
        bookmarked: row.get::<_, i64>("bookmarked")? != 0,
        last_chapter_num: row.get("last_chapter_num")?,
        ffn_updated_at: row.get("ffn_updated_at")?,
        source: row.get("source")?,
        cached_chapter_count: row.get("cached_chapter_count")?,
    })
}

/// The `s.source, s.source_story_id, ...` column list shared by every
/// `StoryRow` query. Architecture review candidate 3 (2026-07-21):
/// `row_to_story` looks these up by name (every expression carries an
/// explicit `AS`), not position — reordering this list can't silently
/// mismap a field the way positional `row.get(N)` could.
const STORY_COLUMNS: &str = "s.id AS id, s.source_story_id AS source_story_id, s.title AS title, s.author AS author,
                s.fandom AS fandom, s.rating AS rating, s.genre AS genre, s.language AS language, s.tags AS tags,
                s.status AS status, s.word_count AS word_count, s.favorite_count AS favorite_count,
                s.chapter_count AS chapter_count, s.summary AS summary,
                COALESCE(rs.is_read, 0) AS is_read, COALESCE(rs.dismissed, 0) AS dismissed,
                COALESCE(rs.bookmarked, 0) AS bookmarked,
                rs.last_chapter_num AS last_chapter_num, s.ffn_updated_at AS ffn_updated_at, s.source AS source,
                (SELECT COUNT(*) FROM chapters c WHERE c.story_id = s.id AND c.content <> '') AS cached_chapter_count";

pub fn list_stories(conn: &Connection, filters: &StoryFilters, scope: Scope) -> rusqlite::Result<Vec<StoryRow>> {
    let mut sql = format!(
        "SELECT {STORY_COLUMNS}
         FROM stories s
         LEFT JOIN read_state rs ON rs.story_id = s.id
         WHERE 1 = 1"
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(fandom) = filters.fandom.as_deref().filter(|f| !f.is_empty()) {
        sql.push_str(" AND s.fandom = ?");
        params.push(Box::new(fandom.to_string()));
    }
    if let Some(source) = filters.source.as_deref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND s.source = ?");
        params.push(Box::new(source.to_string()));
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
    if filters.hide_read {
        sql.push_str(" AND COALESCE(rs.is_read, 0) = 0");
    }
    if filters.hide_dismissed {
        sql.push_str(" AND COALESCE(rs.dismissed, 0) = 0");
    }
    if filters.hide_bookmarked {
        sql.push_str(" AND COALESCE(rs.bookmarked, 0) = 0");
    }
    if scope == Scope::BookmarksOnly {
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

/// Lowercases and strips diacritics so search is accent-insensitive (e.g.
/// "poke" matches "Poké") — NFD-decomposes then drops combining marks
/// (architecture review candidate 7, 2026-07-24), matching
/// `src/lib/normalize.ts`'s `fold()` exactly (fixture-tested in tandem via
/// `tests/fixtures/accent-fold.json`) instead of the hand-coded Latin table
/// this used to be, which covered fewer accents than the TS side and could
/// silently drift further as either side changed.
fn fold(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.to_lowercase().nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect()
}

/// Single story by internal id — the Reader needs this (title, fandom,
/// `source_story_id` for constructing chapter URLs, current read/bookmark
/// state) but doesn't want to run a filtered list query for it.
pub fn get_story(conn: &Connection, story_id: i64) -> rusqlite::Result<Option<StoryRow>> {
    conn.query_row(
        &format!(
            "SELECT {STORY_COLUMNS}
             FROM stories s
             LEFT JOIN read_state rs ON rs.story_id = s.id
             WHERE s.id = ?1"
        ),
        rusqlite::params![story_id],
        row_to_story,
    )
    .map(Some)
    .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })
}

/// Renames a fandom bucket across every story that has it — merges into an
/// existing fandom if `new_fandom` already names one. Exists because a
/// fandom's `stories.fandom` text isn't always consistent: AO3 rows
/// searched before `search_ao3`'s fandom-label override (SPEC.md §8) kept
/// AO3's own canonical tag text (e.g. "Harry Potter - J. K. Rowling")
/// instead of matching the FFN side's shorter name, and AO3 crossover works
/// store a comma-joined multi-fandom string as one bucket (e.g. "Naruto,
/// Game of Thrones (TV), ...") — both need a manual one-time fix-up, not
/// just prevention going forward. Returns how many rows changed.
pub fn rename_fandom(conn: &Connection, old_fandom: &str, new_fandom: &str) -> rusqlite::Result<usize> {
    conn.execute("UPDATE stories SET fandom = ?1 WHERE fandom = ?2", rusqlite::params![new_fandom, old_fandom])
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
    let mut stmt = conn.prepare(&format!(
        "SELECT {STORY_COLUMNS}
         FROM stories s
         JOIN read_state rs ON rs.story_id = s.id
         WHERE rs.last_opened_at IS NOT NULL AND rs.is_read = 0 AND rs.dismissed = 0
         ORDER BY rs.last_opened_at DESC
         LIMIT ?1"
    ))?;
    let rows = stmt.query_map(rusqlite::params![limit], row_to_story)?;
    rows.collect()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterListItem {
    pub chapter_num: i64,
    pub title: String,
    pub is_cached: bool,
    /// AO3's opaque per-chapter id (`chapters.source_chapter_id`, SPEC.md
    /// §8) — the Reader needs this to build a `/works/{id}/chapters/{id}`
    /// fetch URL for an AO3 chapter; always `null` for FFN rows.
    pub source_chapter_id: Option<i64>,
}

/// Full TOC for a story, including chapters whose content hasn't been
/// fetched yet (`is_cached: false`) — the Reader's chapter drawer needs the
/// whole list to be browsable even before every chapter has been opened.
pub fn list_chapters(conn: &Connection, story_id: i64) -> rusqlite::Result<Vec<ChapterListItem>> {
    let mut stmt = conn.prepare(
        "SELECT chapter_num, title, content <> '', source_chapter_id FROM chapters WHERE story_id = ?1 ORDER BY chapter_num",
    )?;
    let rows = stmt.query_map(rusqlite::params![story_id], |row| {
        Ok(ChapterListItem {
            chapter_num: row.get(0)?,
            title: row.get(1)?,
            is_cached: row.get(2)?,
            source_chapter_id: row.get(3)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        crate::db::tests::memory_conn()
    }

    fn sample_ao3(source_story_id: i64) -> crate::ao3::Ao3StoryMetadata {
        crate::ao3::Ao3StoryMetadata {
            source_story_id,
            title: "AO3 Story".to_string(),
            author: "ao3_author".to_string(),
            fandom: "Naruto".to_string(),
            rating: "Explicit".to_string(),
            category: Some("F/M".to_string()),
            warnings: None,
            tags: None,
            language: None,
            status: "in_progress".to_string(),
            word_count: 5000,
            favorite_count: 50,
            chapter_count: 2,
            summary: None,
        }
    }

    fn sample_ffn(ffn_story_id: i64) -> crate::ffn::StoryMetadata {
        crate::ffn::StoryMetadata {
            ffn_story_id,
            title: "FFN Story".to_string(),
            author: "ffn_author".to_string(),
            fandom: "Naruto".to_string(),
            rating: "Fiction T".to_string(),
            genre: None,
            language: None,
            tags: None,
            status: "complete".to_string(),
            word_count: 3000,
            favorite_count: 30,
            chapter_count: 1,
            summary: None,
            updated_at: None,
        }
    }

    #[test]
    fn story_row_reports_correct_source_for_each_source() {
        let conn = test_conn();
        let ffn_id = crate::db::upsert_story(&conn, &sample_ffn(1)).unwrap();
        let ao3_id = crate::db::upsert_ao3_story(&conn, &sample_ao3(2)).unwrap();
        crate::db::ensure_read_state(&conn, ffn_id).unwrap();
        crate::db::ensure_read_state(&conn, ao3_id).unwrap();

        let ffn_row = get_story(&conn, ffn_id).unwrap().unwrap();
        let ao3_row = get_story(&conn, ao3_id).unwrap().unwrap();
        assert_eq!(ffn_row.source, "ffn");
        assert_eq!(ao3_row.source, "ao3");

        let all = list_stories(&conn, &StoryFilters::default(), Scope::All).unwrap();
        assert_eq!(all.len(), 2);

        let ao3_only = list_stories(&conn, &StoryFilters { source: Some("ao3".to_string()), ..Default::default() }, Scope::All).unwrap();
        assert_eq!(ao3_only.len(), 1);
        assert_eq!(ao3_only[0].source, "ao3");
    }

    #[test]
    fn row_to_story_maps_by_name_not_position() {
        // Architecture review candidate 3: row_to_story must keep working
        // against a SELECT whose column order doesn't match STORY_COLUMNS's
        // own order — proves the mapping is name-based, not positional.
        let conn = test_conn();
        let ffn_id = crate::db::upsert_story(&conn, &sample_ffn(1)).unwrap();
        crate::db::ensure_read_state(&conn, ffn_id).unwrap();

        let reordered_sql = "SELECT
                s.title AS title, s.id AS id, s.source AS source, s.source_story_id AS source_story_id,
                s.author AS author, s.fandom AS fandom, s.rating AS rating, s.genre AS genre,
                s.language AS language, s.tags AS tags, s.status AS status, s.word_count AS word_count,
                s.favorite_count AS favorite_count, s.chapter_count AS chapter_count, s.summary AS summary,
                COALESCE(rs.bookmarked, 0) AS bookmarked, COALESCE(rs.is_read, 0) AS is_read,
                COALESCE(rs.dismissed, 0) AS dismissed, rs.last_chapter_num AS last_chapter_num,
                s.ffn_updated_at AS ffn_updated_at,
                (SELECT COUNT(*) FROM chapters c WHERE c.story_id = s.id AND c.content <> '') AS cached_chapter_count
             FROM stories s LEFT JOIN read_state rs ON rs.story_id = s.id WHERE s.id = ?1";

        let row = conn.query_row(reordered_sql, rusqlite::params![ffn_id], row_to_story).unwrap();
        assert_eq!(row.id, ffn_id);
        assert_eq!(row.title, "FFN Story");
        assert_eq!(row.source, "ffn");
        assert_eq!(row.word_count, 3000);
    }

    #[test]
    fn chapter_list_item_exposes_source_chapter_id() {
        let conn = test_conn();
        let ao3_id = crate::db::upsert_ao3_story(&conn, &sample_ao3(2)).unwrap();
        crate::db::upsert_chapter_stub(&conn, ao3_id, 1, "Chapter 1", Some(999)).unwrap();

        let ffn_id = crate::db::upsert_story(&conn, &sample_ffn(1)).unwrap();
        crate::db::upsert_chapter_stub(&conn, ffn_id, 1, "Chapter 1", None).unwrap();

        let ao3_chapters = list_chapters(&conn, ao3_id).unwrap();
        assert_eq!(ao3_chapters[0].source_chapter_id, Some(999));

        let ffn_chapters = list_chapters(&conn, ffn_id).unwrap();
        assert_eq!(ffn_chapters[0].source_chapter_id, None);
    }

    #[test]
    fn rename_fandom_updates_matching_rows_and_merges_into_an_existing_one() {
        let conn = test_conn();
        let mut ao3_meta = sample_ao3(1);
        ao3_meta.fandom = "Harry Potter - J. K. Rowling".to_string();
        crate::db::upsert_ao3_story(&conn, &ao3_meta).unwrap();

        let mut ffn_meta = sample_ffn(2);
        ffn_meta.fandom = "Harry Potter".to_string();
        crate::db::upsert_story(&conn, &ffn_meta).unwrap();

        let changed = rename_fandom(&conn, "Harry Potter - J. K. Rowling", "Harry Potter").unwrap();
        assert_eq!(changed, 1);

        let fandoms = list_fandoms(&conn).unwrap();
        assert_eq!(fandoms, vec!["Harry Potter".to_string()]);

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM stories WHERE fandom = 'Harry Potter'", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn fold_matches_shared_accent_fixture() {
        // Architecture review candidate 7: this fixture is also read by
        // src/lib/normalize.test.ts — both `fold()` adapters must agree on
        // every case, not just their own.
        let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/accent-fold.json"));
        let cases: Vec<(String, String)> = serde_json::from_str(fixture).unwrap();
        for (input, expected) in cases {
            assert_eq!(fold(&input), expected, "fold({input:?})");
        }
    }

    // ---- list_stories / flags / continue-reading over a seeded library ----

    /// Six stories, ids 1..=6 in insertion order. Chosen so each filter
    /// separates a different subset:
    ///
    /// | id | src | fandom  | status      | words  | fav | fetched    | flags                  |
    /// |----|-----|---------|-------------|--------|-----|------------|------------------------|
    /// | 1  | ffn | Naruto  | complete    | 100000 | 500 | 2026-01-01 | read                   |
    /// | 2  | ffn | Bleach  | in_progress | 20000  | 300 | 2026-03-01 | bookmarked             |
    /// | 3  | ao3 | Naruto  | in_progress | 50000  | 900 | 2026-02-01 | dismissed, bookmarked  |
    /// | 4  | ao3 | Bleach  | complete    | 5000   | 100 | 2026-04-01 | read, bookmarked       |
    /// | 5  | ffn | Pokemon | complete    | 70000  | 700 | 2026-05-01 | none                   |
    /// | 6  | ao3 | Naruto  | complete    | 90000  | 400 | 2025-12-01 | no read_state row at all |
    fn seeded() -> Connection {
        let conn = test_conn();
        let stories: [(&str, i64, &str, &str, &str, &str, &str, i64, i64, Option<&str>); 6] = [
            ("ffn", 101, "Alpha", "Ann", "Naruto", "complete", "2026-01-01", 100000, 500, None),
            ("ffn", 102, "Beta", "Bob", "Bleach", "in_progress", "2026-03-01", 20000, 300, None),
            ("ao3", 103, "Gamma", "Cy", "Naruto", "in_progress", "2026-02-01", 50000, 900, None),
            ("ao3", 104, "Delta", "Di", "Bleach", "complete", "2026-04-01", 5000, 100, None),
            ("ffn", 105, "Pokémon Quest", "Zed", "Pokemon", "complete", "2026-05-01", 70000, 700, Some("Gotta catch 'em all")),
            ("ao3", 106, "Zeta", "Émile", "Naruto", "complete", "2025-12-01", 90000, 400, Some("Café au lait")),
        ];
        for (source, sid, title, author, fandom, status, fetched, words, fav, summary) in stories {
            conn.execute(
                "INSERT INTO stories (source, source_story_id, title, author, fandom, rating, status, word_count,
                                      favorite_count, chapter_count, summary, last_fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'T', ?6, ?7, ?8, 1, ?9, ?10)",
                rusqlite::params![source, sid, title, author, fandom, status, words, fav, summary, fetched],
            )
            .unwrap();
        }
        // (story_id, is_read, dismissed, bookmarked)
        for (id, read, dismissed, bookmarked) in [(1, 1, 0, 0), (2, 0, 0, 1), (3, 0, 1, 1), (4, 1, 0, 1), (5, 0, 0, 0)] {
            conn.execute(
                "INSERT INTO read_state (story_id, is_read, dismissed, bookmarked) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, read, dismissed, bookmarked],
            )
            .unwrap();
        }
        conn
    }

    fn ids(conn: &Connection, filters: StoryFilters, scope: Scope) -> Vec<i64> {
        list_stories(conn, &filters, scope).unwrap().into_iter().map(|s| s.id).collect()
    }

    fn filters() -> StoryFilters {
        StoryFilters::default()
    }

    #[test]
    fn list_stories_default_is_everything_by_favorites_desc() {
        assert_eq!(ids(&seeded(), filters(), Scope::All), [3, 5, 1, 6, 2, 4]);
    }

    #[test]
    fn list_stories_sorts_by_words_and_by_last_fetched() {
        let conn = seeded();
        assert_eq!(ids(&conn, StoryFilters { sort_by: "words".into(), ..filters() }, Scope::All), [1, 6, 5, 3, 2, 4]);
        assert_eq!(ids(&conn, StoryFilters { sort_by: "updated".into(), ..filters() }, Scope::All), [5, 4, 2, 3, 1, 6]);
    }

    #[test]
    fn list_stories_scalar_filters_each_narrow_on_their_own() {
        let conn = seeded();
        let fandom = |f: &str| StoryFilters { fandom: Some(f.into()), ..filters() };
        assert_eq!(ids(&conn, fandom("Naruto"), Scope::All), [3, 1, 6]);
        // An empty fandom is "no filter", not "fandom equals empty string".
        assert_eq!(ids(&conn, fandom(""), Scope::All), [3, 5, 1, 6, 2, 4]);
        assert_eq!(ids(&conn, StoryFilters { source: Some("ao3".into()), ..filters() }, Scope::All), [3, 6, 4]);
        assert_eq!(ids(&conn, StoryFilters { status: Some("complete".into()), ..filters() }, Scope::All), [5, 1, 6, 4]);
        assert_eq!(ids(&conn, StoryFilters { min_word_count: Some(50000), ..filters() }, Scope::All), [3, 5, 1, 6]);
    }

    #[test]
    fn list_stories_hide_flags_each_drop_their_rows_and_treat_a_missing_read_state_as_unflagged() {
        let conn = seeded();
        assert_eq!(ids(&conn, StoryFilters { hide_read: true, ..filters() }, Scope::All), [3, 5, 6, 2]);
        assert_eq!(ids(&conn, StoryFilters { hide_dismissed: true, ..filters() }, Scope::All), [5, 1, 6, 2, 4]);
        // Story 6 has no read_state row: it must survive every hide_* filter.
        assert_eq!(ids(&conn, StoryFilters { hide_bookmarked: true, ..filters() }, Scope::All), [5, 1, 6]);
    }

    #[test]
    fn list_stories_filters_combine() {
        let conn = seeded();
        let f = StoryFilters {
            fandom: Some("Naruto".into()),
            status: Some("complete".into()),
            hide_read: true,
            ..filters()
        };
        assert_eq!(ids(&conn, f, Scope::All), [6]);
    }

    #[test]
    fn bookmarks_scope_returns_only_bookmarked_stories_under_every_filter() {
        let conn = seeded();
        assert_eq!(ids(&conn, filters(), Scope::BookmarksOnly), [3, 2, 4]);

        let combos = [
            filters(),
            StoryFilters { hide_read: true, ..filters() },
            StoryFilters { hide_dismissed: true, ..filters() },
            StoryFilters { fandom: Some("Bleach".into()), ..filters() },
            StoryFilters { sort_by: "words".into(), ..filters() },
            StoryFilters { search: Some("a".into()), ..filters() },
        ];
        for combo in combos {
            let label = format!("{combo:?}");
            for story in list_stories(&conn, &combo, Scope::BookmarksOnly).unwrap() {
                assert!(story.bookmarked, "unbookmarked story {} leaked into Bookmarks under {label}", story.id);
            }
        }
        // Contradictory on purpose: bookmarks scope + hide_bookmarked is empty, not "everything".
        assert!(ids(&conn, StoryFilters { hide_bookmarked: true, ..filters() }, Scope::BookmarksOnly).is_empty());
    }

    #[test]
    fn list_stories_search_is_accent_and_case_insensitive_over_title_author_and_summary() {
        let conn = seeded();
        let search = |s: &str| ids(&conn, StoryFilters { search: Some(s.into()), ..filters() }, Scope::All);
        assert_eq!(search("poke"), [5], "title, accent-folded");
        assert_eq!(search("EMILE"), [6], "author, accent-folded and case-folded");
        assert_eq!(search("cafe"), [6], "summary, accent-folded");
        assert_eq!(search("  zed  "), [5], "trimmed");
        assert_eq!(search("naruto"), Vec::<i64>::new(), "fandom is never searched — it has its own picker");
        assert_eq!(search("   "), [3, 5, 1, 6, 2, 4], "blank search is no search");
    }

    #[test]
    fn set_flags_persist_and_stamp_or_clear_their_timestamp() {
        let conn = seeded();
        let stamp = |column: &str| -> Option<String> {
            conn.query_row(&format!("SELECT {column} FROM read_state WHERE story_id = 5"), [], |r| r.get(0)).unwrap()
        };

        set_read(&conn, 5, true).unwrap();
        set_dismissed(&conn, 5, true).unwrap();
        set_bookmarked(&conn, 5, true).unwrap();
        let row = get_story(&conn, 5).unwrap().unwrap();
        assert!(row.is_read && row.dismissed && row.bookmarked);
        assert!(stamp("marked_at").is_some() && stamp("dismissed_at").is_some() && stamp("bookmarked_at").is_some());

        set_read(&conn, 5, false).unwrap();
        let row = get_story(&conn, 5).unwrap().unwrap();
        assert!(!row.is_read && row.dismissed && row.bookmarked, "flags are independent");
        assert!(stamp("marked_at").is_none());
    }

    #[test]
    fn continue_reading_lists_unfinished_opened_stories_newest_first_and_honours_limit() {
        let conn = seeded();
        // 1: read (excluded)  3: dismissed (excluded)  6: no read_state row (excluded)  4: never opened
        for (id, opened) in [(2, "2026-06-01"), (5, "2026-07-01"), (1, "2026-08-01"), (3, "2026-09-01")] {
            conn.execute("UPDATE read_state SET last_opened_at = ?1 WHERE story_id = ?2", rusqlite::params![opened, id])
                .unwrap();
        }
        let ids_of = |limit| -> Vec<i64> {
            get_continue_reading(&conn, limit).unwrap().into_iter().map(|s| s.id).collect()
        };
        assert_eq!(ids_of(10), [5, 2]);
        assert_eq!(ids_of(1), [5]);
    }

    #[test]
    fn set_resume_chapter_stamps_last_opened_so_the_story_shows_up_in_continue_reading() {
        let conn = seeded();
        assert!(get_continue_reading(&conn, 10).unwrap().is_empty());

        set_resume_chapter(&conn, 5, 3).unwrap();

        let rows = get_continue_reading(&conn, 10).unwrap();
        assert_eq!(rows.iter().map(|s| s.id).collect::<Vec<_>>(), [5]);
        assert_eq!(rows[0].last_chapter_num, Some(3));
    }
}
