use rusqlite::Connection;
use std::sync::Mutex;
use tauri::Manager;

/// Schema locked in SPEC.md §5. `IF NOT EXISTS` makes this safe to run on
/// every launch rather than needing a separate migration-tracking system —
/// fine for v1's single-schema-version scope.
const SCHEMA: &str = r#"
-- `source` + `source_story_id` (composite-unique) replace the old
-- FFN-only `ffn_story_id UNIQUE` so a second source (AO3, SPEC.md §8) can
-- coexist without id collisions. `category`/`warnings` are AO3-only,
-- nullable, unused by FFN rows (option (a) from §8: bolt-on columns over a
-- separate per-source tag table, cheap and matches how `ffn_updated_at`
-- was added).
CREATE TABLE IF NOT EXISTS stories (
    id              INTEGER PRIMARY KEY,
    source          TEXT NOT NULL DEFAULT 'ffn' CHECK (source IN ('ffn', 'ao3')),
    source_story_id INTEGER NOT NULL,
    title           TEXT NOT NULL,
    author          TEXT NOT NULL,
    fandom          TEXT NOT NULL,
    rating          TEXT NOT NULL,
    genre           TEXT,
    language        TEXT,
    tags            TEXT,
    category        TEXT,
    warnings        TEXT,
    status          TEXT NOT NULL CHECK (status IN ('complete', 'in_progress')),
    word_count      INTEGER NOT NULL,
    favorite_count  INTEGER NOT NULL,
    chapter_count   INTEGER NOT NULL,
    summary         TEXT,
    ffn_updated_at  TEXT,
    last_fetched_at TEXT NOT NULL,
    UNIQUE (source, source_story_id)
);

CREATE TABLE IF NOT EXISTS chapters (
    id           INTEGER PRIMARY KEY,
    story_id     INTEGER NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    chapter_num  INTEGER NOT NULL,
    title        TEXT,
    content      TEXT NOT NULL,
    fetched_at   TEXT NOT NULL,
    UNIQUE (story_id, chapter_num)
);

CREATE TABLE IF NOT EXISTS read_state (
    story_id         INTEGER PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    is_read          INTEGER NOT NULL DEFAULT 0,
    marked_at        TEXT,
    last_chapter_num INTEGER,
    last_opened_at   TEXT,
    dismissed        INTEGER NOT NULL DEFAULT 0,
    dismissed_at     TEXT,
    bookmarked       INTEGER NOT NULL DEFAULT 0,
    bookmarked_at    TEXT
);

CREATE TABLE IF NOT EXISTS filter_presets (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    filters_json TEXT NOT NULL
);

-- Tracks FFN discovery searches (SPEC.md §6 "Load more") so re-running a
-- search resumes from the next page instead of re-fetching what's already
-- imported. `category` also doubles as the only place a fandom's FFN
-- category is remembered — `stories.fandom` is just a display string.
-- min_word_count/status use sentinel non-null defaults (0 / 'any') rather
-- than NULL — SQLite's UNIQUE treats NULL as distinct from NULL, which would
-- silently break the upsert-by-unique-key for "no filter" searches.
CREATE TABLE IF NOT EXISTS browse_state (
    id                 INTEGER PRIMARY KEY,
    category           TEXT NOT NULL,
    fandom             TEXT NOT NULL,
    min_word_count     INTEGER NOT NULL DEFAULT 0,
    status             TEXT NOT NULL DEFAULT 'any',
    last_page_fetched  INTEGER NOT NULL DEFAULT 0,
    last_fetched_at    TEXT NOT NULL,
    UNIQUE (fandom, min_word_count, status)
);

-- Cached per-category fandom list (FFN's own category index page, e.g.
-- fanfiction.net/anime/) so the "Search FFN" modal offers exact slugs/names
-- to pick from instead of free-text typing. Only fandoms with >= 1,000
-- fics are stored — the long tail below that isn't worth surfacing.
CREATE TABLE IF NOT EXISTS fandom_index (
    category    TEXT NOT NULL,
    fandom      TEXT NOT NULL,
    ffn_count   INTEGER NOT NULL,
    indexed_at  TEXT NOT NULL,
    PRIMARY KEY (category, fandom)
);
"#;

pub struct Db(pub Mutex<Connection>);

/// Upserts a story by its (source, source_story_id) natural key — FFN is the
/// only source that feeds this today, hardcoded below; an AO3 parser will
/// need its own insert path since its metadata shape differs (category,
/// warnings, split relationship/character tags — SPEC.md §8). Re-fetching an
/// existing story updates its cached metadata without touching `read_state`
/// — that's the user's own manual flags, not fetched data, and must survive
/// refetches. Returns the internal `stories.id` (the FK target used
/// everywhere else).
pub fn upsert_story(conn: &Connection, meta: &crate::ffn::StoryMetadata) -> rusqlite::Result<i64> {
    conn.query_row(
        "INSERT INTO stories
            (source, source_story_id, title, author, fandom, rating, genre, language, tags, status,
             word_count, favorite_count, chapter_count, summary, ffn_updated_at, last_fetched_at)
         VALUES ('ffn', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, datetime('now'))
         ON CONFLICT(source, source_story_id) DO UPDATE SET
            title = excluded.title,
            author = excluded.author,
            fandom = excluded.fandom,
            rating = excluded.rating,
            genre = excluded.genre,
            language = excluded.language,
            tags = excluded.tags,
            status = excluded.status,
            word_count = excluded.word_count,
            favorite_count = excluded.favorite_count,
            chapter_count = excluded.chapter_count,
            summary = excluded.summary,
            ffn_updated_at = excluded.ffn_updated_at,
            last_fetched_at = excluded.last_fetched_at
         RETURNING id",
        rusqlite::params![
            meta.ffn_story_id,
            meta.title,
            meta.author,
            meta.fandom,
            meta.rating,
            meta.genre,
            meta.language,
            meta.tags,
            meta.status,
            meta.word_count,
            meta.favorite_count,
            meta.chapter_count,
            meta.summary,
            meta.updated_at,
        ],
        |row| row.get(0),
    )
}

/// Creates the read_state row for a story if it doesn't already have one.
/// `DO NOTHING` on conflict is the point — an existing story's manual
/// is_read/dismissed/bookmarked flags must never be reset by a refetch.
pub fn ensure_read_state(conn: &Connection, story_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO read_state (story_id) VALUES (?1) ON CONFLICT(story_id) DO NOTHING",
        rusqlite::params![story_id],
    )?;
    Ok(())
}

/// Registers a chapter's number/title from the story page's TOC dropdown
/// without touching its content. Every story page (whichever chapter it is)
/// lists the *full* chapter TOC, so calling this for all of them on every
/// fetch keeps chapter titles available for navigation before a chapter's
/// own page has ever been visited. `ON CONFLICT` only touches `title` — an
/// already-cached chapter's `content`/`fetched_at` must survive this.
pub fn upsert_chapter_stub(
    conn: &Connection,
    story_id: i64,
    chapter_num: i64,
    title: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO chapters (story_id, chapter_num, title, content, fetched_at)
         VALUES (?1, ?2, ?3, '', '')
         ON CONFLICT(story_id, chapter_num) DO UPDATE SET title = excluded.title",
        rusqlite::params![story_id, chapter_num, title],
    )?;
    Ok(())
}

/// Fills in a chapter's actual content once its page has been fetched —
/// call after `upsert_chapter_stub` for the specific chapter just navigated to.
pub fn upsert_chapter_content(
    conn: &Connection,
    story_id: i64,
    chapter_num: i64,
    title: &str,
    content: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO chapters (story_id, chapter_num, title, content, fetched_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(story_id, chapter_num) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            fetched_at = excluded.fetched_at",
        rusqlite::params![story_id, chapter_num, title, content],
    )?;
    Ok(())
}

/// Where "Load more" should resume for a given search — the next page to
/// fetch, plus a category hint (needed to reconstruct the browse URL; a
/// fandom's category isn't stored anywhere else). `explicit_category`, when
/// given, is trusted as-is (the "search a new fandom" flow already has it
/// from the user); otherwise falls back to *any* prior search for that
/// fandom, since a fandom's category doesn't vary by filter combo.
pub fn next_browse_page(
    conn: &Connection,
    explicit_category: Option<&str>,
    fandom: &str,
    min_word_count: i64,
    status: &str,
) -> rusqlite::Result<(String, i64)> {
    if let Some(category) = explicit_category {
        let last: Option<i64> = conn
            .query_row(
                "SELECT last_page_fetched FROM browse_state WHERE fandom = ?1 AND min_word_count = ?2 AND status = ?3",
                rusqlite::params![fandom, min_word_count, status],
                |row| row.get(0),
            )
            .ok();
        return Ok((category.to_string(), last.unwrap_or(0) + 1));
    }

    // No explicit category — look up this exact combo first, then any prior
    // search for the fandom (for the category hint) if this exact combo is new.
    if let Some((category, last)) = conn
        .query_row(
            "SELECT category, last_page_fetched FROM browse_state WHERE fandom = ?1 AND min_word_count = ?2 AND status = ?3",
            rusqlite::params![fandom, min_word_count, status],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok()
    {
        return Ok((category, last + 1));
    }
    let category: String = conn.query_row(
        "SELECT category FROM browse_state WHERE fandom = ?1 ORDER BY last_fetched_at DESC LIMIT 1",
        rusqlite::params![fandom],
        |row| row.get(0),
    )?;
    Ok((category, 1))
}

pub fn record_browse_page(
    conn: &Connection,
    category: &str,
    fandom: &str,
    min_word_count: i64,
    status: &str,
    page: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO browse_state (category, fandom, min_word_count, status, last_page_fetched, last_fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(fandom, min_word_count, status) DO UPDATE SET
            category = excluded.category,
            last_page_fetched = MAX(last_page_fetched, excluded.last_page_fetched),
            last_fetched_at = excluded.last_fetched_at",
        rusqlite::params![category, fandom, min_word_count, status, page],
    )?;
    Ok(())
}

/// Replaces a category's cached fandom index wholesale — FFN's own index
/// page is already the full, current list in one fetch, so there's nothing
/// to merge, just swap the old snapshot for the new one.
pub fn replace_fandom_index(conn: &mut Connection, category: &str, entries: &[(String, i64)]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM fandom_index WHERE category = ?1", rusqlite::params![category])?;
    for (fandom, count) in entries {
        tx.execute(
            "INSERT INTO fandom_index (category, fandom, ffn_count, indexed_at) VALUES (?1, ?2, ?3, datetime('now'))",
            rusqlite::params![category, fandom, count],
        )?;
    }
    tx.commit()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FandomIndexEntry {
    pub category: String,
    pub fandom: String,
    pub ffn_count: i64,
}

/// `category: None` searches across every category indexed so far —
/// lets "browse when you don't know what to search for" default to the
/// globally most-popular fandoms rather than requiring a category pick first.
pub fn list_fandom_index(conn: &Connection, category: Option<&str>) -> rusqlite::Result<Vec<FandomIndexEntry>> {
    let row_fn = |row: &rusqlite::Row| {
        Ok(FandomIndexEntry {
            category: row.get(0)?,
            fandom: row.get(1)?,
            ffn_count: row.get(2)?,
        })
    };
    match category {
        Some(cat) => {
            let mut stmt = conn.prepare(
                "SELECT category, fandom, ffn_count FROM fandom_index WHERE category = ?1 ORDER BY ffn_count DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![cat], row_fn)?;
            rows.collect()
        }
        None => {
            let mut stmt = conn.prepare("SELECT category, fandom, ffn_count FROM fandom_index ORDER BY ffn_count DESC")?;
            let rows = stmt.query_map([], row_fn)?;
            rows.collect()
        }
    }
}

/// `CREATE TABLE IF NOT EXISTS` only shapes brand-new databases — it can't
/// add a column to a table that already exists on disk from a previous run.
/// This adds columns introduced after a table's original shape to databases
/// that predate them, guarded by a `pragma_table_info` check so it's a no-op
/// once the column exists (including on brand-new databases, where `SCHEMA`
/// already creates it).
fn add_column_if_missing(conn: &Connection, table: &str, column: &str, ddl_type: &str) -> rusqlite::Result<()> {
    let exists: bool = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"),
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if !exists {
        println!("[db] migrating: adding {table}.{column}");
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl_type}"), [])?;
    }
    Ok(())
}

/// Fixes rows imported before `parse_browse_url` percent-decoded the fandom
/// path segment — non-ASCII fandom names (e.g. "Pokémon") were stored
/// URL-encoded ("Pok%C3%A9mon") in `fandom`/`fandom` columns. One-time
/// repair, guarded by the `%` check so it's a no-op on already-clean rows.
fn fix_percent_encoded_fandoms(conn: &Connection) -> rusqlite::Result<()> {
    for (table, column) in [("stories", "fandom"), ("browse_state", "fandom")] {
        let mut stmt = conn.prepare(&format!("SELECT rowid, {column} FROM {table} WHERE {column} LIKE '%\\%%' ESCAPE '\\'"))?;
        let bad: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        for (rowid, encoded) in bad {
            let decoded = percent_encoding::percent_decode_str(&encoded).decode_utf8_lossy().into_owned();
            if decoded != encoded {
                println!("[db] migrating: decoding {table}.{column} {encoded:?} -> {decoded:?}");
                conn.execute(&format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2"), rusqlite::params![decoded, rowid])?;
            }
        }
    }
    Ok(())
}

/// Rebuilds `stories` to replace the old FFN-only `ffn_story_id UNIQUE` with
/// `source` + `source_story_id UNIQUE(source, source_story_id)` (SPEC.md §8).
/// A straight `ALTER TABLE` can rename/add columns but can't change a UNIQUE
/// constraint, so this does the standard SQLite rebuild: rename the old
/// table aside, create the new shape, copy rows across (all pre-existing
/// rows are 'ffn' by construction — AO3 didn't exist yet), drop the old
/// table. `id` is copied explicitly so `chapters`/`read_state` foreign keys
/// (which point at `stories.id`, not `ffn_story_id`) stay valid untouched.
/// Guarded by a `source` column check so it's a no-op once migrated
/// (including on brand-new databases, where `SCHEMA` already creates the new
/// shape directly). Foreign key enforcement is dropped for the duration —
/// renaming `stories` out from under `chapters`/`read_state` mid-rebuild
/// would otherwise trip FK checks against a momentarily-missing table.
fn migrate_stories_source_column(conn: &Connection) -> rusqlite::Result<()> {
    let already_migrated: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('stories') WHERE name = 'source'",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if already_migrated {
        return Ok(());
    }
    println!("[db] migrating: stories.ffn_story_id -> source + source_story_id");
    conn.pragma_update(None, "foreign_keys", false)?;
    conn.execute_batch(
        "BEGIN;
         ALTER TABLE stories RENAME TO stories_old;
         CREATE TABLE stories (
             id              INTEGER PRIMARY KEY,
             source          TEXT NOT NULL DEFAULT 'ffn' CHECK (source IN ('ffn', 'ao3')),
             source_story_id INTEGER NOT NULL,
             title           TEXT NOT NULL,
             author          TEXT NOT NULL,
             fandom          TEXT NOT NULL,
             rating          TEXT NOT NULL,
             genre           TEXT,
             language        TEXT,
             tags            TEXT,
             category        TEXT,
             warnings        TEXT,
             status          TEXT NOT NULL CHECK (status IN ('complete', 'in_progress')),
             word_count      INTEGER NOT NULL,
             favorite_count  INTEGER NOT NULL,
             chapter_count   INTEGER NOT NULL,
             summary         TEXT,
             ffn_updated_at  TEXT,
             last_fetched_at TEXT NOT NULL,
             UNIQUE (source, source_story_id)
         );
         INSERT INTO stories
             (id, source, source_story_id, title, author, fandom, rating, genre, language, tags,
              status, word_count, favorite_count, chapter_count, summary, ffn_updated_at, last_fetched_at)
         SELECT id, 'ffn', ffn_story_id, title, author, fandom, rating, genre, language, tags,
                status, word_count, favorite_count, chapter_count, summary, ffn_updated_at, last_fetched_at
         FROM stories_old;
         DROP TABLE stories_old;
         COMMIT;",
    )?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    add_column_if_missing(conn, "read_state", "last_opened_at", "TEXT")?;
    add_column_if_missing(conn, "stories", "tags", "TEXT")?;
    add_column_if_missing(conn, "stories", "ffn_updated_at", "TEXT")?;
    fix_percent_encoded_fandoms(conn)?;
    migrate_stories_source_column(conn)?;
    Ok(())
}

pub fn init(app: &tauri::AppHandle) -> rusqlite::Result<Connection> {
    let dir = app
        .path()
        .app_data_dir()
        .expect("could not resolve app data dir");
    std::fs::create_dir_all(&dir).expect("could not create app data dir");
    let db_path = dir.join("longform.db");
    println!("[db] opening {}", db_path.display());

    let conn = Connection::open(&db_path)?;
    // rusqlite does not enable FK enforcement by default; ON DELETE CASCADE
    // in the schema above is a no-op without this.
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}
