---
id: 003
title: Data model / SQLite schema for stories, chapters, read-state, filter presets
label: wayfinder:grilling
status: closed
assignee: claude-session
blocked_by: []
parent: ../map.md
---

## Question

Given the locked filters (mark-as-read, min word count, sort by favorites, status) and the local-caching decision, what's the local data model? Needs tables/fields for: stories (metadata, fandom, rating, word count, favorite count, status), chapters (content, per-story ordering, fetch timestamp for "check for updates"), read-state (per-story manual mark-as-read flag), and saved filter presets if wanted. Use /domain-modeling to pin terminology and shape before writing the schema into the spec.

## Resolution

Added two decisions before finalizing: resume-reading position (per-story last-read chapter) and saved filter presets — both locked in.

```sql
CREATE TABLE stories (
    id              INTEGER PRIMARY KEY,
    ffn_story_id    INTEGER NOT NULL UNIQUE,   -- FFN's own story id, from the URL
    title           TEXT NOT NULL,
    author          TEXT NOT NULL,
    fandom          TEXT NOT NULL,
    rating          TEXT NOT NULL,
    genre           TEXT,
    language        TEXT,
    status          TEXT NOT NULL CHECK (status IN ('complete', 'in_progress')),
    word_count      INTEGER NOT NULL,
    favorite_count  INTEGER NOT NULL,
    chapter_count   INTEGER NOT NULL,
    summary         TEXT,
    last_fetched_at TEXT NOT NULL              -- ISO8601; drives "check for updates"
);

CREATE TABLE chapters (
    id           INTEGER PRIMARY KEY,
    story_id     INTEGER NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    chapter_num  INTEGER NOT NULL,
    title        TEXT,
    content      TEXT NOT NULL,                -- cached chapter HTML/text
    fetched_at   TEXT NOT NULL,
    UNIQUE (story_id, chapter_num)
);

CREATE TABLE read_state (
    story_id        INTEGER PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    is_read         INTEGER NOT NULL DEFAULT 0,   -- manual toggle, boolean
    marked_at       TEXT,
    last_chapter_num INTEGER,                      -- resume-reading position
    dismissed       INTEGER NOT NULL DEFAULT 0,   -- "not for me", separate from is_read
    dismissed_at    TEXT,
    bookmarked      INTEGER NOT NULL DEFAULT 0,   -- browsable in the Bookmarks screen
    bookmarked_at   TEXT
);

CREATE TABLE filter_presets (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    filters_json TEXT NOT NULL   -- serialized: min_word_count, status, sort_by, etc.
);
```

Notes:
- `stories.ffn_story_id` is the natural key tying cached data back to the source; internal `id` is the FK target everywhere else.
- `read_state` is 1:1 with `stories` (PK = story_id) rather than a boolean column on `stories` itself — keeps read/resume state (which changes often) separate from fetched metadata (which changes only on refresh).
- `chapters.content` stores the already-scraped/cleaned text ready for the continuous-scroll reader — no re-parsing on read.

## Amendment

New locked feature: **dismiss** ("not for me") — separate flag from `is_read`, for stories rejected without finishing (e.g. bailed after chapter 1). Added `dismissed` / `dismissed_at` to `read_state` rather than a new table — same shape as `is_read` (manual per-story toggle), and both default to hiding the story from default query results, mirroring the mark-as-read filtering pattern.

## Amendment 2

New locked feature: **bookmark** — surfaced during [ticket #5](005-ui-ux-layout-prototype.md)'s prototype review; a Bookmarks screen browses everything the user has bookmarked, independent of read/dismissed state (a story can be read *and* bookmarked, or bookmarked and untouched). Added `bookmarked` / `bookmarked_at` to `read_state`, same shape as the other two per-story flags — no new table needed, `read_state` is already the home for manual per-story flags.

## Amendment 3

New locked feature: **Continue Reading** — surfaced while building the query/toggle command layer; the Bookmarks screen should be able to show the most recently-read in-progress story. This needed a real schema addition, not just a query: nothing previously tracked *when* a story was last opened (`marked_at` only fires on the is_read toggle; `last_chapter_num` gives position but not recency). Added `last_opened_at TEXT` to `read_state`, stamped by `set_resume_chapter` every time the Reader lands on a chapter — reuses the existing resume-position hook rather than adding a new one, since both fire on the same "user is actively reading this" event. Because this table already existed on disk from earlier scaffolding, `db::init` also gained a small one-off migration (`ALTER TABLE ... ADD COLUMN`, guarded by a `pragma_table_info` check) rather than relying solely on `CREATE TABLE IF NOT EXISTS`, which only shapes brand-new databases.

## Empirical validation (post-map, during scaffolding)

Schema applied via `rusqlite` on app startup (`app/src-tauri/src/db.rs`, `CREATE TABLE IF NOT EXISTS`, `PRAGMA foreign_keys = ON`). Confirmed live in the running app: all four tables (`stories`, `chapters`, `read_state`, `filter_presets`) exist with 0 rows each on a fresh database, via a `db_health_check` command wired to a "Check DB" button in the scaffold UI.
