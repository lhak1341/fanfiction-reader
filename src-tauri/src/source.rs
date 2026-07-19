//! `FicSource` seam (architecture review candidate 1, 2026-07-19): a single
//! dispatch surface for the two operations FFN and AO3 both support —
//! fetching a chapter, and fetching a browse/listing page. Parsing itself
//! stays fully source-specific (`ffn.rs`/`ao3.rs` untouched, only called
//! into from here); this only removes the duplication in *how*
//! `lib.rs::try_persist_fetch` dispatched to each one. FFN's category-index
//! fetch has no AO3 equivalent and deliberately stays outside this trait,
//! handled directly in `lib.rs` (SPEC.md §8). The DB upsert layer
//! (`db::upsert_story` / `db::upsert_ao3_story`) and pagination bookkeeping
//! (`db::record_browse_page` / `db::record_ao3_browse_page`) also stay as
//! two functions each, per SPEC.md §8's existing call that the two
//! metadata shapes don't overlap enough to share a schema-facing signature
//! — `persist_story`/`record_listing_page` below just wrap them.

use rusqlite::Connection;

use crate::{ao3, db, ffn};

/// Minimal chapter-shape accessor so the generic dispatch in `lib.rs` can
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

    /// `Ok(None)` means "not this kind of URL, try the next dispatch arm" —
    /// distinct from `Err`, which means "this *is* the shape, but it's
    /// malformed" and should be propagated rather than swallowed as a miss.
    fn chapter_locator_from_url(url: &str) -> Result<Option<Self::ChapterLocator>, String>;
    fn listing_locator_from_url(url: &str) -> Option<Self::ListingLocator>;

    fn parse_work(html: &str, locator: &Self::ChapterLocator) -> Result<(Self::Metadata, Vec<Self::ChapterRef>), String>;
    fn parse_listing(html: &str, locator: &Self::ListingLocator) -> Result<Vec<Self::Metadata>, String>;
    fn parse_chapter_content(html: &str) -> Result<String, String>;

    fn resolve_chapter_num(chapters: &[Self::ChapterRef], locator: &Self::ChapterLocator) -> Result<i64, String>;

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
}

pub struct Ffn;

pub struct FfnChapterLocator {
    pub story_id: i64,
    pub chapter_num: i64,
}

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

    fn chapter_locator_from_url(url: &str) -> Result<Option<Self::ChapterLocator>, String> {
        match (ffn::story_id_from_url(url), ffn::chapter_num_from_url(url)) {
            (Ok(story_id), Ok(chapter_num)) => Ok(Some(FfnChapterLocator { story_id, chapter_num })),
            _ => Ok(None),
        }
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
}

pub struct Ao3;

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

    fn chapter_locator_from_url(url: &str) -> Result<Option<Self::ChapterLocator>, String> {
        let Ok(work_id) = ao3::work_id_from_url(url) else { return Ok(None) };
        let chapter_id = ao3::chapter_id_from_url(url)?;
        Ok(Some(Ao3ChapterLocator { work_id, chapter_id }))
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
}
