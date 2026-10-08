//! Schema migrations: one-off, self-guarding repairs run on every launch by
//! `db::init`. The whole interface is `migrate(conn)`; each entry in
//! `MIGRATIONS` checks for its own precondition (column exists, LIKE pattern,
//! `pragma_foreign_key_list`, ...) before acting, so none needs a
//! `schema_version` table (see the declined-candidate note above `MIGRATIONS`).
//! Split out of `db.rs` (architecture review candidate 4, 2026-10-07) so the
//! repair history stops sharing a file with the live query/upsert interface.
//! `SCHEMA` and `init` stay in `db.rs`.

use rusqlite::Connection;

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

/// AO3's `<h3 class="landmark heading" id="work">Chapter Text</h3>` — a fixed
/// screen-reader landmark label, not per-chapter content — leaked into
/// `chapters.content` for AO3 rows fetched before `ao3::parse_chapter_content`
/// started stripping it (2026-07-19); it showed up as a redundant heading
/// above the real prose in the Reader. One-time repair for rows fetched
/// before the fix; new fetches are unaffected. Guarded by the `LIKE` check
/// so it's a no-op on already-clean rows (including brand-new fetches).
fn fix_ao3_landmark_heading_in_chapters(conn: &Connection) -> rusqlite::Result<()> {
    const NEEDLE: &str = "<h3 class=\"landmark heading\" id=\"work\">Chapter Text</h3>";
    let mut stmt = conn.prepare(
        "SELECT c.id, c.content FROM chapters c
         JOIN stories s ON s.id = c.story_id
         WHERE s.source = 'ao3' AND c.content LIKE '%Chapter Text%'",
    )?;
    let rows: Vec<(i64, String)> = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for (id, content) in rows {
        let Some(idx) = content.find(NEEDLE) else { continue };
        let mut cleaned = content;
        cleaned.replace_range(idx..idx + NEEDLE.len(), "");
        println!("[db] migrating: stripping AO3 landmark heading from chapters.id={id}");
        conn.execute("UPDATE chapters SET content = ?1 WHERE id = ?2", rusqlite::params![cleaned, id])?;
    }
    Ok(())
}

// Every entry here is independently self-guarding (checks column existence,
// a LIKE pattern, pragma_foreign_key_list, etc. before acting) and safe to
// re-run on every launch — see SCHEMA's doc comment on why that means no
// schema_version table is needed at this scope. This list exists so
// `migrate` can't grow another hardcoded call that's easy to forget; add a
// new migration by adding an entry here.
//
// Architecture review candidate 4 (2026-07-29, declined): each entry here is
// permanent — nothing ever removes an old one-off repair once satisfied,
// since `db::init` has no record of which repairs a given database has
// already absorbed. Reopened the no-`schema_version` decision above and
// declined to change it: 8 entries, each a one-liner plus a cheap
// self-guarding function, isn't unbounded growth in practice yet, and
// nothing here is broken or untested. Revisit only if this list itself
// becomes the pain point — not before.
const MIGRATIONS: &[fn(&Connection) -> rusqlite::Result<()>] = &[
    |c| add_column_if_missing(c, "read_state", "last_opened_at", "TEXT"),
    |c| add_column_if_missing(c, "read_state", "last_scroll_pct", "INTEGER"),
    |c| add_column_if_missing(c, "stories", "tags", "TEXT"),
    |c| add_column_if_missing(c, "stories", "ffn_updated_at", "TEXT"),
    |c| add_column_if_missing(c, "chapters", "source_chapter_id", "INTEGER"),
    fix_percent_encoded_fandoms,
    migrate_stories_source_column,
    fix_dangling_stories_fk,
    fix_ao3_landmark_heading_in_chapters,
];

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    for f in MIGRATIONS {
        f(conn)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{sample_ao3_story, schema_conn, temp_conn};
    use crate::db::{upsert_ao3_story, upsert_chapter_content, upsert_story, SCHEMA};

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
    fn migrate_adds_last_scroll_pct_to_an_existing_read_state_and_keeps_its_rows() {
        let conn = crate::db::tests::memory_conn();
        conn.execute_batch(
            "INSERT INTO stories (id, source, source_story_id, title, author, fandom, rating, status, word_count,
                 favorite_count, chapter_count, last_fetched_at)
             VALUES (1, 'ffn', 11, 't', 'a', 'f', 'K', 'complete', 1, 1, 1, '2026-01-01');
             INSERT INTO read_state (story_id, last_chapter_num) VALUES (1, 7);
             ALTER TABLE read_state DROP COLUMN last_scroll_pct;",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let (chapter, pct): (i64, Option<i64>) = conn
            .query_row("SELECT last_chapter_num, last_scroll_pct FROM read_state WHERE story_id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((chapter, pct), (7, None), "an existing resume point reopens at the top, not lost");
        migrate(&conn).unwrap(); // idempotent
    }

    #[test]
    fn fix_dangling_stories_fk_is_noop_on_healthy_db() {
        let conn = temp_conn("healthy");
        conn.execute_batch(SCHEMA).unwrap();
        fix_dangling_stories_fk(&conn).unwrap();
        assert_eq!(dangling_stories_fk_count(&conn), 0);
    }

    #[test]
    fn fix_ao3_landmark_heading_in_chapters_strips_leaked_heading_and_preserves_prose() {
        let conn = schema_conn("ao3_landmark_heading");
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
        upsert_chapter_content(
            &conn,
            ao3_id,
            1,
            "Chapter 1",
            "\n    <h3 class=\"landmark heading\" id=\"work\">Chapter Text</h3>\n    <p>Real prose.</p>",
            None,
        )
        .unwrap();
        // An FFN chapter that innocently contains the literal phrase (not the
        // landmark tag) must survive untouched — the fix targets the exact
        // AO3 heading element, not the phrase "Chapter Text" anywhere.
        upsert_chapter_content(&conn, ffn_id, 1, "Chapter 1", "<p>He typed \"Chapter Text\" as a joke.</p>", None).unwrap();

        fix_ao3_landmark_heading_in_chapters(&conn).unwrap();

        let ao3_content: String = conn.query_row("SELECT content FROM chapters WHERE story_id = ?1 AND chapter_num = 1", [ao3_id], |row| row.get(0)).unwrap();
        assert_eq!(ao3_content, "\n    \n    <p>Real prose.</p>");
        assert!(!ao3_content.contains("Chapter Text"));

        let ffn_content: String = conn.query_row("SELECT content FROM chapters WHERE story_id = ?1 AND chapter_num = 1", [ffn_id], |row| row.get(0)).unwrap();
        assert_eq!(ffn_content, "<p>He typed \"Chapter Text\" as a joke.</p>");

        // Idempotent: already-clean rows are left alone on a second run.
        fix_ao3_landmark_heading_in_chapters(&conn).unwrap();
        let ao3_content_again: String = conn.query_row("SELECT content FROM chapters WHERE story_id = ?1 AND chapter_num = 1", [ao3_id], |row| row.get(0)).unwrap();
        assert_eq!(ao3_content_again, ao3_content);
    }
}
