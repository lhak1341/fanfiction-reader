use rusqlite::Connection;
use std::sync::Mutex;
use tauri::Manager;

/// Schema locked in SPEC.md §5. `IF NOT EXISTS` makes this safe to run on
/// every launch rather than needing a separate migration-tracking system —
/// fine for v1's single-schema-version scope.
pub(crate) const SCHEMA: &str = r#"
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
    id                INTEGER PRIMARY KEY,
    story_id          INTEGER NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    chapter_num       INTEGER NOT NULL,
    title             TEXT,
    content           TEXT NOT NULL,
    fetched_at        TEXT NOT NULL,
    source_chapter_id INTEGER,          -- AO3 only: opaque per-chapter id from /works/{id}/chapters/{chapter_id} (SPEC.md §8) — NULL for FFN
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

-- AO3 counterpart to browse_state (SPEC.md §8) — simpler than FFN's since
-- AO3 has no category concept to resolve, just a tag. Same sentinel
-- non-null defaults (0 / 'any') for the same reason as browse_state: SQLite
-- UNIQUE treats NULL as distinct from NULL, which would break the upsert
-- for "no filter" searches.
CREATE TABLE IF NOT EXISTS ao3_browse_state (
    id                 INTEGER PRIMARY KEY,
    tag                TEXT NOT NULL,
    min_word_count     INTEGER NOT NULL DEFAULT 0,
    status             TEXT NOT NULL DEFAULT 'any',
    last_page_fetched  INTEGER NOT NULL DEFAULT 0,
    last_fetched_at    TEXT NOT NULL,
    UNIQUE (tag, min_word_count, status)
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

/// AO3 counterpart to `upsert_story` — separate function rather than a
/// shared one taking `source` explicitly (SPEC.md §8's other option) since
/// the two metadata shapes don't overlap enough to make a shared signature
/// worth it: AO3 has `category`/`warnings` and no `genre`/`ffn_updated_at`
/// concept, FFN is the reverse. Same re-fetch contract as `upsert_story` —
/// `read_state` is untouched, `ON CONFLICT` only refreshes cached metadata.
pub fn upsert_ao3_story(conn: &Connection, meta: &crate::ao3::Ao3StoryMetadata) -> rusqlite::Result<i64> {
    conn.query_row(
        "INSERT INTO stories
            (source, source_story_id, title, author, fandom, rating, language, tags, category, warnings,
             status, word_count, favorite_count, chapter_count, summary, last_fetched_at)
         VALUES ('ao3', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, datetime('now'))
         ON CONFLICT(source, source_story_id) DO UPDATE SET
            title = excluded.title,
            author = excluded.author,
            fandom = excluded.fandom,
            rating = excluded.rating,
            language = excluded.language,
            tags = excluded.tags,
            category = excluded.category,
            warnings = excluded.warnings,
            status = excluded.status,
            word_count = excluded.word_count,
            favorite_count = excluded.favorite_count,
            chapter_count = excluded.chapter_count,
            summary = excluded.summary,
            last_fetched_at = excluded.last_fetched_at
         RETURNING id",
        rusqlite::params![
            meta.source_story_id,
            meta.title,
            meta.author,
            meta.fandom,
            meta.rating,
            meta.language,
            meta.tags,
            meta.category,
            meta.warnings,
            meta.status,
            meta.word_count,
            meta.favorite_count,
            meta.chapter_count,
            meta.summary,
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
/// own page has ever been visited. `ON CONFLICT` only touches `title`/
/// `source_chapter_id` — an already-cached chapter's `content`/`fetched_at`
/// must survive this. `source_chapter_id` is FFN's own chapter number
/// re-used as AO3's opaque per-chapter id would be meaningless for FFN
/// (which addresses chapters by number in the URL, §4) — FFN callers pass
/// `None`; AO3 callers pass the `chapter_id` needed to build
/// `/works/{id}/chapters/{chapter_id}` (§8).
pub fn upsert_chapter_stub(
    conn: &Connection,
    story_id: i64,
    chapter_num: i64,
    title: &str,
    source_chapter_id: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO chapters (story_id, chapter_num, title, content, fetched_at, source_chapter_id)
         VALUES (?1, ?2, ?3, '', '', ?4)
         ON CONFLICT(story_id, chapter_num) DO UPDATE SET
            title = excluded.title,
            source_chapter_id = excluded.source_chapter_id",
        rusqlite::params![story_id, chapter_num, title, source_chapter_id],
    )?;
    Ok(())
}

/// Fills in a chapter's actual content once its page has been fetched —
/// call after `upsert_chapter_stub` for the specific chapter just navigated
/// to. See `upsert_chapter_stub` on `source_chapter_id`.
pub fn upsert_chapter_content(
    conn: &Connection,
    story_id: i64,
    chapter_num: i64,
    title: &str,
    content: &str,
    source_chapter_id: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO chapters (story_id, chapter_num, title, content, fetched_at, source_chapter_id)
         VALUES (?1, ?2, ?3, ?4, datetime('now'), ?5)
         ON CONFLICT(story_id, chapter_num) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            fetched_at = excluded.fetched_at,
            source_chapter_id = excluded.source_chapter_id",
        rusqlite::params![story_id, chapter_num, title, content, source_chapter_id],
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

/// AO3 counterpart to `next_browse_page` — no category to resolve (AO3 has
/// no category concept), so this is just a straight lookup: 1 if this exact
/// tag+filter combo has never been fetched, otherwise one past whatever was
/// last fetched.
pub fn next_ao3_browse_page(conn: &Connection, tag: &str, min_word_count: i64, status: &str) -> rusqlite::Result<i64> {
    let last: Option<i64> = conn
        .query_row(
            "SELECT last_page_fetched FROM ao3_browse_state WHERE tag = ?1 AND min_word_count = ?2 AND status = ?3",
            rusqlite::params![tag, min_word_count, status],
            |row| row.get(0),
        )
        .ok();
    Ok(last.unwrap_or(0) + 1)
}

pub fn record_ao3_browse_page(conn: &Connection, tag: &str, min_word_count: i64, status: &str, page: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO ao3_browse_state (tag, min_word_count, status, last_page_fetched, last_fetched_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(tag, min_word_count, status) DO UPDATE SET
            last_page_fetched = MAX(last_page_fetched, excluded.last_page_fetched),
            last_fetched_at = excluded.last_fetched_at",
        rusqlite::params![tag, min_word_count, status, page],
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
    // Without this, `RENAME TO stories_old` makes SQLite auto-rewrite every
    // other table's FK clause that pointed at `stories` (e.g. `chapters`,
    // `read_state`) to point at `stories_old` instead — so once `stories_old`
    // is dropped below, those tables are left referencing a table that no
    // longer exists (silently breaks `ON DELETE CASCADE`). `legacy_alter_table`
    // disables that auto-rewrite, so their FK text stays `stories` throughout
    // and correctly resolves to the new `stories` table created here.
    conn.pragma_update(None, "legacy_alter_table", true)?;
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
    conn.pragma_update(None, "legacy_alter_table", false)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

/// One-time repair for DBs that already ran the pre-fix
/// `migrate_stories_source_column` (which left `chapters`/`read_state`
/// referencing a dropped `stories_old` table — see the comment there).
/// Detects the dangling reference via `pragma_foreign_key_list` (its `table`
/// column names the FK's target) rather than a version flag, so it's a
/// no-op once repaired and harmless on DBs that never had the bug.
fn fix_dangling_stories_fk(conn: &Connection) -> rusqlite::Result<()> {
    for table in ["chapters", "read_state"] {
        let dangling: bool = conn.query_row(
            &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}') WHERE \"table\" = 'stories_old'"),
            [],
            |row| row.get::<_, i64>(0),
        )? > 0;
        if !dangling {
            continue;
        }
        println!("[db] migrating: repairing {table}'s dangling FK to stories_old");
        let columns: Vec<String> = {
            let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?;
            let names = stmt.query_map([], |row| row.get::<_, String>(0))?;
            names.collect::<rusqlite::Result<_>>()?
        };
        let create_sql: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            rusqlite::params![table],
            |row| row.get(0),
        )?;
        let fixed_create_sql = create_sql.replacen("\"stories_old\"", "stories", 1);
        let col_list = columns.join(", ");
        conn.pragma_update(None, "foreign_keys", false)?;
        conn.execute_batch(&format!(
            "BEGIN;
             ALTER TABLE {table} RENAME TO {table}_old;
             {fixed_create_sql};
             INSERT INTO {table} ({col_list}) SELECT {col_list} FROM {table}_old;
             DROP TABLE {table}_old;
             COMMIT;"
        ))?;
        conn.pragma_update(None, "foreign_keys", true)?;
    }
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    add_column_if_missing(conn, "read_state", "last_opened_at", "TEXT")?;
    add_column_if_missing(conn, "stories", "tags", "TEXT")?;
    add_column_if_missing(conn, "stories", "ffn_updated_at", "TEXT")?;
    add_column_if_missing(conn, "chapters", "source_chapter_id", "INTEGER")?;
    fix_percent_encoded_fandoms(conn)?;
    migrate_stories_source_column(conn)?;
    fix_dangling_stories_fk(conn)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Reproduces the dangling-FK corruption without the fix present:
    /// `RENAME TO stories_old` auto-rewrites `chapters`/`read_state`'s FK
    /// clause to follow the rename (default SQLite behavior), then dropping
    /// `stories_old` leaves them pointing at a table that no longer exists.
    /// This is the exact sequence the pre-fix `migrate_stories_source_column`
    /// ran against the real production DB.
    fn corrupt_stories_fk(conn: &Connection) {
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute_batch(
            "CREATE TABLE stories (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE chapters (
                 id INTEGER PRIMARY KEY,
                 story_id INTEGER NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
                 content TEXT NOT NULL
             );
             CREATE TABLE read_state (
                 story_id INTEGER PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
                 is_read INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO stories (id, name) VALUES (1, 'Test Story');
             INSERT INTO chapters (id, story_id, content) VALUES (10, 1, 'chapter text');
             INSERT INTO read_state (story_id, is_read) VALUES (1, 1);
             ALTER TABLE stories RENAME TO stories_old;
             CREATE TABLE stories (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO stories SELECT * FROM stories_old;
             DROP TABLE stories_old;",
        )
        .unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
    }

    /// A temp-file-backed connection rather than `:memory:` — SQLite's
    /// `pragma_foreign_key_check` was observed to behave inconsistently
    /// in-memory when a referenced table is entirely missing (silently 0
    /// violations, or in some schema shapes a hard "no such table" error,
    /// depending on schema details) but reliably reports real violations
    /// against an on-disk DB, which is what production actually uses.
    fn temp_conn(name: &str) -> Connection {
        let path = std::env::temp_dir().join(format!("longform_db_test_{name}_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Connection::open(&path).unwrap()
    }

    fn dangling_stories_fk_count(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM (
                SELECT 1 FROM pragma_foreign_key_list('chapters') WHERE \"table\" = 'stories_old'
                UNION ALL
                SELECT 1 FROM pragma_foreign_key_list('read_state') WHERE \"table\" = 'stories_old'
             )",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn fix_dangling_stories_fk_repairs_corrupted_db_and_preserves_data() {
        let conn = temp_conn("repair");
        corrupt_stories_fk(&conn);

        // Confirm the corruption actually reproduced before testing the fix.
        assert!(dangling_stories_fk_count(&conn) > 0, "test setup didn't reproduce the dangling FK");

        fix_dangling_stories_fk(&conn).unwrap();

        assert_eq!(dangling_stories_fk_count(&conn), 0);
        let violations_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(violations_after, 0);

        let chapter_content: String = conn
            .query_row("SELECT content FROM chapters WHERE id = 10", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chapter_content, "chapter text");

        let is_read: i64 = conn
            .query_row("SELECT is_read FROM read_state WHERE story_id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(is_read, 1);

        // Idempotent: running it again on an already-fixed DB is a no-op.
        fix_dangling_stories_fk(&conn).unwrap();
    }

    #[test]
    fn fix_dangling_stories_fk_is_noop_on_healthy_db() {
        let conn = temp_conn("healthy");
        conn.execute_batch(SCHEMA).unwrap();
        fix_dangling_stories_fk(&conn).unwrap();
        assert_eq!(dangling_stories_fk_count(&conn), 0);
    }

    fn schema_conn(name: &str) -> Connection {
        let conn = temp_conn(name);
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn sample_ao3_story(source_story_id: i64) -> crate::ao3::Ao3StoryMetadata {
        crate::ao3::Ao3StoryMetadata {
            source_story_id,
            title: "Perks of Being Kidnapped".to_string(),
            author: "some_author".to_string(),
            fandom: "Naruto".to_string(),
            rating: "Explicit".to_string(),
            category: Some("F/M".to_string()),
            warnings: Some("Creator Chose Not To Use Archive Warnings".to_string()),
            tags: Some("Naruto Uzumaki/Hinata Hyuga, Naruto Uzumaki, kidnapping".to_string()),
            language: Some("English".to_string()),
            status: "in_progress".to_string(),
            word_count: 42_000,
            favorite_count: 315,
            chapter_count: 12,
            summary: Some("A summary.".to_string()),
        }
    }

    #[test]
    fn upsert_ao3_story_inserts_ao3_shaped_row() {
        let conn = schema_conn("ao3_insert");
        let meta = sample_ao3_story(85954646);

        let id = upsert_ao3_story(&conn, &meta).unwrap();

        let (source, source_story_id, category, warnings, genre, ffn_updated_at): (
            String,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT source, source_story_id, category, warnings, genre, ffn_updated_at FROM stories WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .unwrap();
        assert_eq!(source, "ao3");
        assert_eq!(source_story_id, 85954646);
        assert_eq!(category.as_deref(), Some("F/M"));
        assert_eq!(warnings.as_deref(), Some("Creator Chose Not To Use Archive Warnings"));
        assert_eq!(genre, None, "AO3 has no genre concept");
        assert_eq!(ffn_updated_at, None, "AO3 has no ffn_updated_at concept");
    }

    #[test]
    fn upsert_ao3_story_refetch_updates_metadata_but_not_read_state() {
        let conn = schema_conn("ao3_refetch");
        let mut meta = sample_ao3_story(85954646);

        let id = upsert_ao3_story(&conn, &meta).unwrap();
        ensure_read_state(&conn, id).unwrap();
        conn.execute("UPDATE read_state SET bookmarked = 1 WHERE story_id = ?1", rusqlite::params![id]).unwrap();

        meta.word_count = 50_000;
        meta.chapter_count = 14;
        let id_again = upsert_ao3_story(&conn, &meta).unwrap();
        assert_eq!(id, id_again, "same (source, source_story_id) must resolve to the same row");

        let (word_count, chapter_count): (i64, i64) = conn
            .query_row("SELECT word_count, chapter_count FROM stories WHERE id = ?1", rusqlite::params![id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(word_count, 50_000);
        assert_eq!(chapter_count, 14);

        let bookmarked: i64 = conn
            .query_row("SELECT bookmarked FROM read_state WHERE story_id = ?1", rusqlite::params![id], |row| row.get(0))
            .unwrap();
        assert_eq!(bookmarked, 1, "refetch must not reset manual read_state flags");
    }

    #[test]
    fn ffn_and_ao3_stories_with_same_source_story_id_coexist() {
        let conn = schema_conn("coexist");
        let ffn_meta = crate::ffn::StoryMetadata {
            ffn_story_id: 42,
            title: "FFN Story".to_string(),
            author: "ffn_author".to_string(),
            fandom: "Naruto".to_string(),
            rating: "Fiction T".to_string(),
            genre: Some("Adventure".to_string()),
            language: Some("English".to_string()),
            tags: Some("Naruto U.".to_string()),
            status: "complete".to_string(),
            word_count: 1000,
            favorite_count: 10,
            chapter_count: 1,
            summary: Some("An FFN summary.".to_string()),
            updated_at: None,
        };
        let ao3_meta = sample_ao3_story(42);

        let ffn_id = upsert_story(&conn, &ffn_meta).unwrap();
        let ao3_id = upsert_ao3_story(&conn, &ao3_meta).unwrap();

        assert_ne!(ffn_id, ao3_id, "(source, source_story_id) must distinguish the two rows, not just source_story_id");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM stories WHERE source_story_id = 42", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn chapter_upserts_carry_source_chapter_id_for_ao3_and_stay_null_for_ffn() {
        let conn = schema_conn("chapter_source_id");
        let ao3_id = upsert_ao3_story(&conn, &sample_ao3_story(1)).unwrap();
        let ffn_id = upsert_story(
            &conn,
            &crate::ffn::StoryMetadata {
                ffn_story_id: 2,
                title: "FFN Story".to_string(),
                author: "a".to_string(),
                fandom: "Naruto".to_string(),
                rating: "Fiction T".to_string(),
                genre: None,
                language: None,
                tags: None,
                status: "complete".to_string(),
                word_count: 100,
                favorite_count: 1,
                chapter_count: 1,
                summary: None,
                updated_at: None,
            },
        )
        .unwrap();

        upsert_chapter_stub(&conn, ao3_id, 1, "Chapter One", Some(227238471)).unwrap();
        upsert_chapter_content(&conn, ao3_id, 1, "Chapter One", "<p>ao3 text</p>", Some(227238471)).unwrap();
        upsert_chapter_stub(&conn, ffn_id, 1, "Chapter One", None).unwrap();
        upsert_chapter_content(&conn, ffn_id, 1, "Chapter One", "<p>ffn text</p>", None).unwrap();

        let ao3_source_chapter_id: Option<i64> = conn
            .query_row(
                "SELECT source_chapter_id FROM chapters WHERE story_id = ?1 AND chapter_num = 1",
                rusqlite::params![ao3_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ao3_source_chapter_id, Some(227238471));

        let ffn_source_chapter_id: Option<i64> = conn
            .query_row(
                "SELECT source_chapter_id FROM chapters WHERE story_id = ?1 AND chapter_num = 1",
                rusqlite::params![ffn_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ffn_source_chapter_id, None);
    }

    #[test]
    fn ao3_browse_page_resume_starts_at_one_and_advances() {
        let conn = schema_conn("ao3_browse_page");
        assert_eq!(next_ao3_browse_page(&conn, "Naruto", 100_000, "complete").unwrap(), 1);

        record_ao3_browse_page(&conn, "Naruto", 100_000, "complete", 1).unwrap();
        assert_eq!(next_ao3_browse_page(&conn, "Naruto", 100_000, "complete").unwrap(), 2);

        record_ao3_browse_page(&conn, "Naruto", 100_000, "complete", 2).unwrap();
        assert_eq!(next_ao3_browse_page(&conn, "Naruto", 100_000, "complete").unwrap(), 3);

        // A different filter combo for the same tag tracks its own page independently.
        assert_eq!(next_ao3_browse_page(&conn, "Naruto", 0, "any").unwrap(), 1);
    }

    #[test]
    fn record_ao3_browse_page_never_regresses_on_out_of_order_calls() {
        let conn = schema_conn("ao3_browse_page_regress");
        record_ao3_browse_page(&conn, "Naruto", 0, "any", 5).unwrap();
        record_ao3_browse_page(&conn, "Naruto", 0, "any", 2).unwrap();
        assert_eq!(next_ao3_browse_page(&conn, "Naruto", 0, "any").unwrap(), 6);
    }
}