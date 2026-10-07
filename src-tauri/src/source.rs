//! `FicSource` seam (architecture review candidate 1, 2026-07-19): a single
//! dispatch surface for the two operations FFN and AO3 both support —
//! fetching a chapter, and fetching a browse/listing page. Parsing itself
//! stays fully source-specific (`ffn.rs`/`ao3.rs` untouched, only called
//! into from here); this only removes the duplication in *how*
//! `dispatch::persist_page` dispatches to each one. FFN's category-index
//! shape has no AO3 equivalent, so it enters the trait only as two
//! default-`None`/empty hooks (`category_from_index_url`/`parse_category_index`),
//! never overridden by `Ao3` (SPEC.md §8). The DB upsert layer
//! (`db::upsert_story` / `db::upsert_ao3_story`) and pagination bookkeeping
//! (`db::record_browse_page` / `db::record_ao3_browse_page`) also stay as
//! two functions each, per SPEC.md §8's existing call that the two
//! metadata shapes don't overlap enough to share a schema-facing signature
//! — `persist_story`/`record_listing_page` below just wrap them.
//!
//! Architecture review candidate 4 (2026-10-07), twin-cursor half rejected:
//! `db::next_browse_page`/`next_ao3_browse_page` (the "Load more" cursor
//! reads, called only from `search_ffn`/`search_ao3` in `fetch.rs`) stay
//! outside this trait too. Their signatures differ for real reasons — FFN
//! resolves a category from an optional hint and returns `(category, page)`,
//! AO3 has no category and returns just `page` — so a shared method would
//! need an associated request type wider than either twin, for two call
//! sites. Reopen if a third source appears.

use rusqlite::Connection;

use crate::{ao3, db, ffn};

/// Minimal chapter-shape accessor so the generic dispatch in `dispatch.rs` can
/// walk a `Vec<S::ChapterRef>` without knowing which concrete struct it holds.
pub trait ChapterInfo {
    fn chapter_num(&self) -> i64;
    fn title(&self) -> &str;
    fn source_chapter_id(&self) -> Option<i64>;
}

impl ChapterInfo for ffn::ChapterRef {
    fn chapter_num(&self) -> i64 {
        self.chapter_num
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn source_chapter_id(&self) -> Option<i64> {
        None
    }
}

impl ChapterInfo for ao3::Ao3ChapterRef {
    fn chapter_num(&self) -> i64 {
        self.chapter_num
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn source_chapter_id(&self) -> Option<i64> {
        self.chapter_id
    }
}

/// One fic source's parsing + fetch-dispatch shape.
pub trait FicSource {
    type Metadata;
    type ChapterRef: ChapterInfo;
    type ChapterLocator;
    type ListingLocator;

    /// Matches `stories.source` (`db.rs` schema) — lets `dispatch::resolve_target`
    /// (`dispatch.rs`) resolve a source's own site id to Longform's internal DB id
    /// generically instead of hand-matching a runtime `FetchSource` enum.
    const SOURCE_TAG: &'static str;

    /// Prefix for this source's `println!`/`eprintln!` lines in `dispatch.rs`/
    /// `fetch.rs` — replaces the runtime `FetchSource` enum that existed only
    /// to pick between these two strings.
    const LOG_PREFIX: &'static str;

    /// `Some(category)` when `url` is this source's category-index page — FFN
    /// only (SPEC.md §8), so the default is `None` and `Ao3` never overrides
    /// it. Checked *first* by `dispatch::classify_page`, ahead of chapter and
    /// listing, so no other shape's check can swallow it.
    fn category_from_index_url(_url: &str) -> Option<String> {
        None
    }

    /// Parses a category-index page into `(fandom, count)` rows. Only called
    /// for a URL `category_from_index_url` matched, so the empty default is
    /// unreachable for `Ao3`.
    fn parse_category_index(_html: &str) -> Vec<(String, i64)> {
        Vec::new()
    }

    /// `Ok(None)` means "not this kind of URL, try the next dispatch arm" —
    /// distinct from `Err`, which means "this *is* the shape, but it's
    /// malformed" and should be propagated rather than swallowed as a miss.
    fn chapter_locator_from_url(url: &str) -> Result<Option<Self::ChapterLocator>, String>;
    fn listing_locator_from_url(url: &str) -> Option<Self::ListingLocator>;

    fn parse_work(html: &str, locator: &Self::ChapterLocator) -> Result<(Self::Metadata, Vec<Self::ChapterRef>), String>;
    fn parse_listing(html: &str, locator: &Self::ListingLocator) -> Result<Vec<Self::Metadata>, String>;
    fn parse_chapter_content(html: &str) -> Result<String, String>;

    fn resolve_chapter_num(chapters: &[Self::ChapterRef], locator: &Self::ChapterLocator) -> Result<i64, String>;

    /// `(source_story_id, chapter_num)` — normalizes each source's own
    /// `ChapterLocator` field names (`story_id`/`chapter_num` vs
    /// `work_id`/`chapter_id`) so `dispatch::resolve_target` can read a chapter
    /// locator generically. `chapter_num` is a display/DB value, not
    /// necessarily what a URL encoded — AO3 locators without an explicit
    /// chapter id (a bare `/works/{id}` URL) report `0`, matching
    /// `dispatch::resolve_target`'s existing behavior of treating that as "no
    /// specific chapter known yet" for lookup purposes.
    fn chapter_locator_ids(locator: &Self::ChapterLocator) -> (i64, i64);

    /// The URL that fetches chapter `chapter_num` of the story whose own-site
    /// id is `source_story_id`. `source_chapter_id` is the per-chapter opaque
    /// id a prior fetch recorded (`chapters.source_chapter_id`), when known —
    /// AO3 chapters beyond the first need it (`/works/{id}/chapters/{id}`,
    /// SPEC.md §8) and fall back to the bare `/works/{id}` page without one;
    /// FFN ignores it.
    fn chapter_url(source_story_id: i64, chapter_num: i64, source_chapter_id: Option<i64>) -> String;

    fn metadata_title(meta: &Self::Metadata) -> &str;
    fn metadata_fandom(meta: &Self::Metadata) -> &str;

    /// No-op by default — FFN derives fandom from the URL itself and is
    /// never called with an override. AO3 overrides this to force-apply the
    /// user's typed fandom label to every row of a tag-listing fetch (see
    /// SPEC.md §8, "Fandom bucket mismatch").
    fn apply_fandom_override(_meta: &mut Self::Metadata, _label: &str) {}

    fn persist_story(conn: &Connection, meta: &Self::Metadata) -> rusqlite::Result<i64>;

    fn listing_page(locator: &Self::ListingLocator) -> i64;
    fn record_listing_page(conn: &Connection, locator: &Self::ListingLocator, page: i64) -> rusqlite::Result<()>;

    /// Which locator (if any) `record_listing_page` should persist under —
    /// `decoded` came from re-parsing the fetched page's own URL, `external`
    /// from whatever the caller stashed before the fetch began (`fetch.rs`'s
    /// `PendingFetch`, FFN-only). `Ffn` prefers `external` and returns `None`
    /// (skip recording) when nothing was stashed — its own URL encoding
    /// can't round-trip `status` losslessly (`build_browse_url`, architecture
    /// review candidate, 2026-07-29), so a bare decode risks persisting a
    /// wrong value; a debug harness's raw-URL fetch has no real search
    /// session behind it worth bookkeeping anyway. `Ao3`'s encoding round-trips
    /// cleanly and it never stashes anything, so it always keeps `decoded`.
    fn resolve_listing_record_locator(decoded: Self::ListingLocator, external: Option<Self::ListingLocator>) -> Option<Self::ListingLocator>;

    /// The fandom `dispatch::resolve_target` should report for a `Browse`
    /// target, given the locator decoded from the URL and (`Ao3` only) the
    /// caller's own fandom hint. `Ffn`'s listing URL encodes fandom directly,
    /// so it ignores `fandom_hint` and always returns `decoded`'s. `Ao3`'s
    /// listing URL only encodes its search *tag*, not the force-applied
    /// fandom label callers actually care about (`apply_fandom_override`), so
    /// it ignores `decoded` entirely and returns `fandom_hint` verbatim —
    /// `None` (no hint given) means there's nothing to recover fandom from,
    /// the accepted degenerate case (`dispatch::resolve_target` falls back to
    /// `Unknown`).
    fn resolve_target_fandom(decoded: Self::ListingLocator, fandom_hint: Option<&str>) -> Option<String>;
}

pub struct Ffn;

#[derive(Debug)]
pub struct FfnChapterLocator {
    pub story_id: i64,
    pub chapter_num: i64,
}

/// `Deserialize` so `lib.rs::fetch_page` (a `#[tauri::command]`) can accept
/// this as a param — always `None` over the wire in practice (only
/// `search_ffn`'s own in-process call ever supplies `Some`, see
/// `PendingFetch`), but the IPC macro still needs the bound to compile.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfnListingLocator {
    pub category: String,
    pub fandom: String,
    pub min_word_count: i64,
    pub status: String,
    pub page: i64,
}

impl FicSource for Ffn {
    type Metadata = ffn::StoryMetadata;
    type ChapterRef = ffn::ChapterRef;
    type ChapterLocator = FfnChapterLocator;
    type ListingLocator = FfnListingLocator;

    const SOURCE_TAG: &'static str = "ffn";
    const LOG_PREFIX: &'static str = "[fetch]";

    fn category_from_index_url(url: &str) -> Option<String> {
        ffn::category_from_index_url(url)
    }

    fn parse_category_index(html: &str) -> Vec<(String, i64)> {
        ffn::parse_fandom_index(html, 1000)
    }

    fn chapter_locator_from_url(url: &str) -> Result<Option<Self::ChapterLocator>, String> {
        Ok(ffn::ffn_locator_from_url(url)?.map(|(story_id, chapter_num)| FfnChapterLocator { story_id, chapter_num }))
    }

    fn listing_locator_from_url(url: &str) -> Option<Self::ListingLocator> {
        let (category, fandom, page) = ffn::parse_browse_url(url)?;
        let parsed_url = url::Url::parse(url).ok()?;
        let min_word_count: i64 = parsed_url
            .query_pairs()
            .find(|(k, _)| k == "len")
            .and_then(|(_, v)| v.parse::<i64>().ok())
            .map(|thousands| thousands * 1000)
            .unwrap_or(0);
        let status = if parsed_url.query_pairs().any(|(k, v)| k == "s" && v == "2") {
            "complete"
        } else {
            "any"
        }
        .to_string();
        Some(FfnListingLocator { category, fandom, min_word_count, status, page })
    }

    fn parse_work(html: &str, locator: &Self::ChapterLocator) -> Result<(Self::Metadata, Vec<Self::ChapterRef>), String> {
        ffn::parse_story_page(html, locator.story_id)
    }

    fn parse_listing(html: &str, locator: &Self::ListingLocator) -> Result<Vec<Self::Metadata>, String> {
        ffn::parse_browse_page(html, &locator.fandom)
    }

    fn parse_chapter_content(html: &str) -> Result<String, String> {
        ffn::parse_chapter_content(html)
    }

    fn resolve_chapter_num(_chapters: &[Self::ChapterRef], locator: &Self::ChapterLocator) -> Result<i64, String> {
        Ok(locator.chapter_num)
    }

    fn chapter_locator_ids(locator: &Self::ChapterLocator) -> (i64, i64) {
        (locator.story_id, locator.chapter_num)
    }

    fn chapter_url(source_story_id: i64, chapter_num: i64, _source_chapter_id: Option<i64>) -> String {
        format!("https://www.fanfiction.net/s/{source_story_id}/{chapter_num}/")
    }

    fn metadata_title(meta: &Self::Metadata) -> &str {
        &meta.title
    }

    fn metadata_fandom(meta: &Self::Metadata) -> &str {
        &meta.fandom
    }

    fn persist_story(conn: &Connection, meta: &Self::Metadata) -> rusqlite::Result<i64> {
        db::upsert_story(conn, meta)
    }

    fn listing_page(locator: &Self::ListingLocator) -> i64 {
        locator.page
    }

    fn record_listing_page(conn: &Connection, locator: &Self::ListingLocator, page: i64) -> rusqlite::Result<()> {
        db::record_browse_page(conn, &locator.category, &locator.fandom, locator.min_word_count, &locator.status, page)
    }

    fn resolve_listing_record_locator(_decoded: Self::ListingLocator, external: Option<Self::ListingLocator>) -> Option<Self::ListingLocator> {
        external
    }

    fn resolve_target_fandom(decoded: Self::ListingLocator, _fandom_hint: Option<&str>) -> Option<String> {
        Some(decoded.fandom)
    }
}

pub struct Ao3;

#[derive(Debug)]
pub struct Ao3ChapterLocator {
    pub work_id: i64,
    pub chapter_id: Option<i64>,
}

pub struct Ao3ListingLocator {
    pub tag: String,
    pub min_word_count: i64,
    pub status: String,
    pub page: i64,
}

impl FicSource for Ao3 {
    type Metadata = ao3::Ao3StoryMetadata;
    type ChapterRef = ao3::Ao3ChapterRef;
    type ChapterLocator = Ao3ChapterLocator;
    type ListingLocator = Ao3ListingLocator;

    const SOURCE_TAG: &'static str = "ao3";
    const LOG_PREFIX: &'static str = "[ao3-fetch]";

    fn chapter_locator_from_url(url: &str) -> Result<Option<Self::ChapterLocator>, String> {
        Ok(ao3::ao3_locator_from_url(url)?.map(|(work_id, chapter_id)| Ao3ChapterLocator { work_id, chapter_id }))
    }

    fn listing_locator_from_url(url: &str) -> Option<Self::ListingLocator> {
        let tag = ao3::tag_from_listing_url(url)?;
        let parsed_url = url::Url::parse(url).ok()?;
        let min_word_count: i64 = parsed_url
            .query_pairs()
            .find(|(k, _)| k == "work_search[words_from]")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let status = match parsed_url.query_pairs().find(|(k, _)| k == "work_search[complete]").map(|(_, v)| v.into_owned()) {
            Some(v) if v == "T" => "complete",
            Some(v) if v == "F" => "in_progress",
            _ => "any",
        }
        .to_string();
        let page: i64 = parsed_url.query_pairs().find(|(k, _)| k == "page").and_then(|(_, v)| v.parse().ok()).unwrap_or(1);
        Some(Ao3ListingLocator { tag, min_word_count, status, page })
    }

    fn parse_work(html: &str, locator: &Self::ChapterLocator) -> Result<(Self::Metadata, Vec<Self::ChapterRef>), String> {
        ao3::parse_work_page(html, locator.work_id)
    }

    fn parse_listing(html: &str, _locator: &Self::ListingLocator) -> Result<Vec<Self::Metadata>, String> {
        ao3::parse_listing_page(html)
    }

    fn parse_chapter_content(html: &str) -> Result<String, String> {
        ao3::parse_chapter_content(html)
    }

    fn resolve_chapter_num(chapters: &[Self::ChapterRef], locator: &Self::ChapterLocator) -> Result<i64, String> {
        match locator.chapter_id {
            Some(id) => chapters
                .iter()
                .find(|c| c.chapter_id == Some(id))
                .map(|c| c.chapter_num)
                .ok_or_else(|| "fetched chapter id not found in this work's own TOC".to_string()),
            None if chapters.len() == 1 => Ok(1),
            None => Err("multi-chapter work URL had no chapter id — expected a redirect to one".to_string()),
        }
    }

    fn chapter_locator_ids(locator: &Self::ChapterLocator) -> (i64, i64) {
        (locator.work_id, locator.chapter_id.unwrap_or(0))
    }

    fn chapter_url(source_story_id: i64, _chapter_num: i64, source_chapter_id: Option<i64>) -> String {
        match source_chapter_id {
            Some(id) => format!("https://archiveofourown.org/works/{source_story_id}/chapters/{id}"),
            None => format!("https://archiveofourown.org/works/{source_story_id}"),
        }
    }

    fn metadata_title(meta: &Self::Metadata) -> &str {
        &meta.title
    }

    fn metadata_fandom(meta: &Self::Metadata) -> &str {
        &meta.fandom
    }

    fn apply_fandom_override(meta: &mut Self::Metadata, label: &str) {
        meta.fandom = label.to_string();
    }

    fn persist_story(conn: &Connection, meta: &Self::Metadata) -> rusqlite::Result<i64> {
        db::upsert_ao3_story(conn, meta)
    }

    fn listing_page(locator: &Self::ListingLocator) -> i64 {
        locator.page
    }

    fn record_listing_page(conn: &Connection, locator: &Self::ListingLocator, page: i64) -> rusqlite::Result<()> {
        db::record_ao3_browse_page(conn, &locator.tag, locator.min_word_count, &locator.status, page)
    }

    fn resolve_listing_record_locator(decoded: Self::ListingLocator, _external: Option<Self::ListingLocator>) -> Option<Self::ListingLocator> {
        Some(decoded)
    }

    fn resolve_target_fandom(_decoded: Self::ListingLocator, fandom_hint: Option<&str>) -> Option<String> {
        fandom_hint.map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Architecture review candidate 1 (2026-07-21): chapter_locator_from_url
    // must propagate a malformed-but-shape-matching URL's real parse error
    // instead of collapsing it into Ok(None).
    #[test]
    fn ffn_chapter_locator_from_url_propagates_malformed_error() {
        let err = Ffn::chapter_locator_from_url("https://www.fanfiction.net/s/not-a-number/1/Title").unwrap_err();
        assert!(err.contains("story id segment"), "unexpected error: {err}");
    }

    #[test]
    fn ao3_chapter_locator_from_url_propagates_malformed_error() {
        let err = Ao3::chapter_locator_from_url("https://archiveofourown.org/works/not-a-number").unwrap_err();
        assert!(err.contains("work id segment"), "unexpected error: {err}");
    }
}
