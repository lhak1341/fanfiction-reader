//! AO3 (archiveofourown.org) extraction logic (SPEC.md §8) — a second source
//! alongside `ffn.rs`. Written against real markup captured from live work
//! and tag-scoped listing pages, not guessed from AO3's public API docs or
//! third-party scrapers, same standard `ffn.rs` holds itself to (§4).
//!
//! This module is the parsing layer only — the upsert path
//! (`db::upsert_ao3_story`) and fetch path (`fetch_ao3_html`/
//! `try_persist_ao3_fetch` in `lib.rs`, plain `reqwest`, no webview needed)
//! live elsewhere, same split `ffn.rs`'s own parsing/fetch layers have.

use scraper::{ElementRef, Html, Selector};

#[derive(Debug, serde::Serialize)]
pub struct Ao3StoryMetadata {
    pub source_story_id: i64,
    pub title: String,
    pub author: String,
    /// Comma-joined if the work lists more than one fandom (AO3 crossovers
    /// are natively multi-fandom, unlike FFN's one-fandom-per-story model —
    /// open question in SPEC.md §8 on whether `fandom` should become a set;
    /// punted on here the same way, just joining rather than picking one).
    pub fandom: String,
    /// AO3's single rating value verbatim (e.g. "Explicit", "Teen And Up
    /// Audiences"), read from the tag's own text/title rather than the
    /// `rating-*` CSS class slug — avoids having to reverse a slug back into
    /// AO3's exact display string.
    pub rating: String,
    /// Comma-joined category tags verbatim (e.g. "F/F, F/M, Multi") — AO3
    /// allows more than one, unlike FFN which has no category concept at
    /// all. `None` on listing-derived rows if the work has no category tag
    /// (rare but possible) or if the summary badge (listing pages only show
    /// one combined badge) is all that's available yet.
    pub category: Option<String>,
    /// Comma-joined warning tags verbatim, including AO3's own placeholder
    /// values ("Creator Chose Not To Use Archive Warnings", "No Archive
    /// Warnings Apply") — those are themselves taggable warning values on
    /// AO3, not an absence of data.
    pub warnings: Option<String>,
    /// Relationship + character + freeform tags, comma-joined into one
    /// string — SPEC.md §8 locked (a): splitting these into their own
    /// columns is deferred until there's an actual second source to prove
    /// the shape is needed; FFN-style mushed `tags` is good enough for now.
    pub tags: Option<String>,
    pub language: Option<String>,
    pub status: String, // "complete" | "in_progress"
    pub word_count: i64,
    /// AO3 bookmark count — SPEC.md §8 locked decision: AO3's bookmarks
    /// (a deliberate save/track action) is the closer analog to FFN's
    /// Favorites, not kudos (a near-frictionless one-click "like"). Sources
    /// are never merged into one ranking; this just reuses the column FFN's
    /// Favs already occupies, scoped per-source at the query layer.
    pub favorite_count: i64,
    /// Chapters actually posted so far (the "N" in AO3's "N/M" chapters
    /// stat) — mirrors FFN's `chapter_count` semantics (§4): how many
    /// chapters are available to fetch right now, not the planned total.
    pub chapter_count: i64,
    pub summary: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct Ao3ChapterRef {
    pub chapter_num: i64,
    pub title: String,
    /// AO3 chapter URLs are `/works/{work_id}/chapters/{chapter_id}` — an
    /// opaque per-chapter id, *not* the sequential chapter number FFN uses
    /// in its `/s/{story_id}/{n}/` URLs. `None` means a one-shot: the work
    /// has no chapter-select dropdown at all, and its one chapter is served
    /// directly at `/works/{work_id}` with no `/chapters/{id}` sub-path.
    pub chapter_id: Option<i64>,
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

fn parse_word_count(s: &str) -> i64 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

/// Joins an element's matched children's text into one comma-separated
/// string, `None` if there were none — used for every AO3 tag group
/// (warnings/category/fandom/relationships/characters/freeforms), which are
/// all rendered the same way: a `ul.commas`/`ul.tags.commas` of `<li><a
/// class="tag">`.
fn join_tag_texts<'a>(root: &ElementRef, sel: &Selector) -> Option<String> {
    let texts: Vec<String> = root.select(sel).map(|el| text_of(&el)).collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join(", "))
    }
}

/// AO3 work URLs look like `/works/85954646` or
/// `/works/85954646/chapters/227238471` — the id is always the second path
/// segment.
pub fn work_id_from_url(url: &str) -> Result<i64, String> {
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    let id = parsed
        .path_segments()
        .and_then(|mut segs| {
            segs.next(); // "works"
            segs.next()
        })
        .ok_or("URL did not contain a work id")?;
    id.parse().map_err(|_| format!("work id segment {id:?} was not numeric"))
}

/// `/works/{id}/chapters/{chapter_id}` — `None` for a bare `/works/{id}`
/// URL (one-shot, or "entire work" view).
pub fn chapter_id_from_url(url: &str) -> Result<Option<i64>, String> {
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    let mut segs = parsed.path_segments().ok_or("URL had no path")?;
    segs.next(); // "works"
    segs.next().ok_or("URL did not contain a work id")?;
    match (segs.next(), segs.next()) {
        (Some("chapters"), Some(id)) => {
            id.parse().map(Some).map_err(|_| format!("chapter id segment {id:?} was not numeric"))
        }
        _ => Ok(None),
    }
}

/// Parses a single AO3 work page (e.g. `https://archiveofourown.org/works/85954646`,
/// whichever chapter it redirected to) into its metadata plus the full
/// chapter list. Confirmed against live markup (2026-07-19) for a
/// multi-chapter Explicit-rated work — see SPEC.md §8.
pub fn parse_work_page(html: &str, work_id: i64) -> Result<(Ao3StoryMetadata, Vec<Ao3ChapterRef>), String> {
    let doc = Html::parse_document(html);

    let workskin_sel = Selector::parse("#workskin").unwrap();
    let workskin = doc
        .select(&workskin_sel)
        .next()
        .ok_or("could not find #workskin — page may not be a work page")?;

    let title_sel = Selector::parse("h2.title.heading").unwrap();
    let title = workskin
        .select(&title_sel)
        .next()
        .map(|e| text_of(&e))
        .ok_or("could not find work title")?;

    let author_sel = Selector::parse(r#"h3.byline.heading a[rel="author"]"#).unwrap();
    let author = workskin
        .select(&author_sel)
        .map(|e| text_of(&e))
        .collect::<Vec<_>>()
        .join(", ");

    let summary_sel = Selector::parse("div.summary.module blockquote.userstuff").unwrap();
    let summary = workskin.select(&summary_sel).next().map(|e| text_of(&e)).filter(|s| !s.is_empty());

    // Queried against the whole document, not scoped under `workskin` like
    // title/author/summary above: confirmed live that `dl.work.meta.group`
    // sits outside the parse tree rooted at `#workskin` (html5ever closes
    // `#workskin` earlier than a lenient browser's rendered nesting would
    // suggest — some unbalanced tag upstream). Safe globally since a work
    // page only ever has one work's worth of this block.
    let meta_sel = Selector::parse("dl.work.meta.group").unwrap();
    let meta = doc.select(&meta_sel).next().ok_or("could not find the work meta (tags) block")?;

    let rating_sel = Selector::parse("dd.rating.tags a.tag").unwrap();
    let rating = join_tag_texts(&meta, &rating_sel).unwrap_or_default();

    let warning_sel = Selector::parse("dd.warning.tags a.tag").unwrap();
    let warnings = join_tag_texts(&meta, &warning_sel);

    let category_sel = Selector::parse("dd.category.tags a.tag").unwrap();
    let category = join_tag_texts(&meta, &category_sel);

    let fandom_sel = Selector::parse("dd.fandom.tags a.tag").unwrap();
    let fandom = join_tag_texts(&meta, &fandom_sel).unwrap_or_default();

    let relationship_sel = Selector::parse("dd.relationship.tags a.tag").unwrap();
    let character_sel = Selector::parse("dd.character.tags a.tag").unwrap();
    let freeform_sel = Selector::parse("dd.freeform.tags a.tag").unwrap();
    let tags = [
        join_tag_texts(&meta, &relationship_sel),
        join_tag_texts(&meta, &character_sel),
        join_tag_texts(&meta, &freeform_sel),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let tags = if tags.is_empty() { None } else { Some(tags.join(", ")) };

    let language_sel = Selector::parse("dd.language").unwrap();
    let language = meta.select(&language_sel).next().map(|e| text_of(&e)).filter(|s| !s.is_empty());

    // The stats block is a *nested* `<dl class="stats">` inside this same
    // meta group's `dd.stats` — not a sibling. `words`/`chapters`/`bookmarks`
    // live there; `published`/`updated_at` are read but not stored yet (no
    // generic updated-at column for AO3 rows — SPEC.md §5 only has FFN's).
    let stats_sel = Selector::parse("dl.stats").unwrap();
    let stats = meta.select(&stats_sel).next().ok_or("could not find the nested stats block")?;

    let words_sel = Selector::parse("dd.words").unwrap();
    let word_count = stats.select(&words_sel).next().map(|e| parse_word_count(&text_of(&e))).unwrap_or(0);

    let bookmarks_sel = Selector::parse("dd.bookmarks").unwrap();
    let favorite_count = stats.select(&bookmarks_sel).next().map(|e| parse_word_count(&text_of(&e))).unwrap_or(0);

    let chapters_stat_sel = Selector::parse("dd.chapters").unwrap();
    let chapters_stat = stats.select(&chapters_stat_sel).next().map(|e| text_of(&e)).unwrap_or_default();
    // "80/?" (ongoing) or "12/12" (complete) — posted-so-far is always the
    // part before the slash.
    let (posted_str, planned_str) = chapters_stat.split_once('/').unwrap_or((chapters_stat.as_str(), ""));
    let posted = parse_word_count(posted_str).max(1);
    let status = if planned_str.trim() == "?" || planned_str.trim() != posted_str.trim() {
        "in_progress"
    } else {
        "complete"
    }
    .to_string();

    // Chapter TOC: `<select id="selected_id"><option value="{chapter_id}">
    // {n}. {title}</option>...</select>`, present only for multi-chapter
    // works — absent entirely for one-shots (see `Ao3ChapterRef::chapter_id`).
    let select_sel = Selector::parse("select#selected_id option").unwrap();
    let mut chapters: Vec<Ao3ChapterRef> = doc
        .select(&select_sel)
        .enumerate()
        .filter_map(|(i, opt)| {
            let chapter_id: i64 = opt.value().attr("value")?.parse().ok()?;
            let raw = text_of(&opt);
            let title = raw.split_once('.').map(|(_, rest)| rest.trim().to_string()).unwrap_or(raw);
            Some(Ao3ChapterRef { chapter_num: (i + 1) as i64, title, chapter_id: Some(chapter_id) })
        })
        .collect();
    if chapters.is_empty() {
        chapters.push(Ao3ChapterRef { chapter_num: 1, title: title.clone(), chapter_id: None });
    }

    let metadata = Ao3StoryMetadata {
        source_story_id: work_id,
        title,
        author,
        fandom,
        rating,
        category,
        warnings,
        tags,
        language,
        status,
        word_count,
        favorite_count,
        chapter_count: posted.max(chapters.len() as i64),
        summary,
    };

    Ok((metadata, chapters))
}

/// Extracts a single chapter's body. The chapter text lives in a
/// `<div class="userstuff module" role="article">` — the `role="article"`
/// attribute is what distinguishes it from the *other* `.userstuff` divs on
/// the same page (author's notes before/after the chapter use plain
/// `class="userstuff"` with no `role`), confirmed against live markup.
pub fn parse_chapter_content(html: &str) -> Result<String, String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"div.userstuff.module[role="article"]"#).unwrap();
    doc.select(&sel)
        .next()
        .map(|el| el.inner_html())
        .ok_or_else(|| "could not find the chapter's userstuff article div".to_string())
}

/// Recognizes a listing URL built by `build_tag_browse_url`
/// (`/works?tag_id={tag}&...`) and recovers the (decoded) tag — used to
/// dispatch a fetched page to `parse_listing_page` vs `parse_work_page`
/// without re-deriving the tag from the listing markup itself (unlike FFN's
/// browse rows, which repeat nothing about the search itself). Matches on
/// the bare `/works` path (no id segment) plus a `tag_id` query param, so it
/// can't collide with `work_id_from_url`'s `/works/{id}` shape.
pub fn tag_from_listing_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let mut segs = parsed.path_segments()?;
    let (Some("works"), None) = (segs.next(), segs.next()) else { return None };
    parsed.query_pairs().find(|(k, _)| k == "tag_id").map(|(_, v)| v.into_owned())
}

/// Builds an AO3 listing URL scoped to one tag via `tag_id`, sortable and
/// filterable by word-count/completion-status.
///
/// **Deliberately crosses a scope line locked earlier in SPEC.md §8**: the
/// tag-scoped `/tags/{tag}/works` path is the only browse shape `robots.txt`
/// allows for crawlers; the global `/works` path (used here, scoped only by
/// the `tag_id` query param) is disallowed there. Confirmed live
/// (2026-07-19) that `/tags/{tag}/works` does *not* apply `work_search[...]`
/// params at all — the filter sidebar's own form (`#work-filters`) posts to
/// `action="/works"` with `tag_id` as a hidden field, which is what a human
/// clicking "Sort and Filter" on a tag page actually submits; there's no
/// server-side-filtered shape that stays inside the tag-scoped path. Decided
/// (2026-07-19, user-confirmed) to use `/works?tag_id=...` anyway, same
/// personal-use-scale posture already applied to FFN's Cloudflare/ToS
/// situation (§1) — this mirrors exactly what a human's browser sends when
/// filtering a tag search, just via `reqwest` instead of a click.
///
/// Confirmed live: `work_search[sort_column]` reorders results (tested
/// `kudos_count` against the unsorted default); `page` is AO3's own
/// pagination param; `work_search[words_from]` is a minimum word count
/// (tested `1000` against `100000` — the latter returned only 6-digit+ word
/// counts); `work_search[complete]` is `"T"` for complete-only or `"F"` for
/// in-progress-only (tested both — verified via each result's own
/// `span.complete-yes/complete-no.iswip` badge, not just absence of the
/// other), omitted entirely for "any" (mirrors FFN's `s=2`-or-omit pattern,
/// §4). No max-word-count or "any" `words_from` — mirrors FFN's `len`
/// (minimum-only, §4).
pub fn build_tag_browse_url(fandom_tag: &str, sort_column: &str, min_word_count: i64, status: &str, page: i64) -> String {
    let encoded_tag = url::form_urlencoded::byte_serialize(fandom_tag.as_bytes()).collect::<String>();
    let mut url = format!(
        "https://archiveofourown.org/works?tag_id={encoded_tag}&work_search%5Bsort_column%5D={sort_column}&page={page}"
    );
    if min_word_count > 0 {
        url.push_str(&format!("&work_search%5Bwords_from%5D={min_word_count}"));
    }
    match status {
        "complete" => url.push_str("&work_search%5Bcomplete%5D=T"),
        "in_progress" => url.push_str("&work_search%5Bcomplete%5D=F"),
        _ => {}
    }
    url
}

/// Parses an AO3 tag-scoped work listing page (e.g.
/// `https://archiveofourown.org/works?tag_id=Naruto`, see
/// `build_tag_browse_url`) into one
/// [`Ao3StoryMetadata`] per work shown. Unlike the full work page, listing
/// rows carry no `dl.work.meta.group` — rating/category/status come from the
/// `ul.required-tags` summary badges instead (one value each, read via each), and full warning/relationship/character/freeform detail comes from the separate `ul.tags.commas` block. `chapter_count` is the
/// posted-so-far count from `dd.chapters`, same semantics as the work page.
pub fn parse_listing_page(html: &str) -> Result<Vec<Ao3StoryMetadata>, String> {
    let doc = Html::parse_document(html);

    let row_sel = Selector::parse("li.work").unwrap();
    let title_sel = Selector::parse(r#"h4.heading a[href^="/works/"]"#).unwrap();
    let author_sel = Selector::parse(r#"h4.heading a[rel="author"]"#).unwrap();
    let fandom_sel = Selector::parse("h5.fandoms.heading a.tag").unwrap();
    let rating_sel = Selector::parse("span.rating").unwrap();
    let category_sel = Selector::parse("span.category").unwrap();
    let complete_sel = Selector::parse("span.iswip").unwrap();
    let warnings_sel = Selector::parse("li.warnings a.tag").unwrap();
    let relationship_sel = Selector::parse("li.relationships a.tag").unwrap();
    let character_sel = Selector::parse("li.characters a.tag").unwrap();
    let freeform_sel = Selector::parse("li.freeforms a.tag").unwrap();
    let summary_sel = Selector::parse("blockquote.userstuff.summary").unwrap();
    let language_sel = Selector::parse("dd.language").unwrap();
    let words_sel = Selector::parse("dd.words").unwrap();
    let bookmarks_sel = Selector::parse("dd.bookmarks").unwrap();
    let chapters_sel = Selector::parse("dd.chapters").unwrap();

    let mut out = Vec::new();

    for row in doc.select(&row_sel) {
        let Some(title_el) = row.select(&title_sel).next() else { continue };
        let Some(source_story_id) = title_el
            .value()
            .attr("href")
            .and_then(|href| href.trim_start_matches("/works/").split('/').next())
            .and_then(|seg| seg.parse::<i64>().ok())
        else {
            continue;
        };
        let title = text_of(&title_el);
        let author = row
            .select(&author_sel)
            .map(|e| text_of(&e))
            .collect::<Vec<_>>()
            .join(", ");

        let fandom = join_tag_texts(&row, &fandom_sel).unwrap_or_default();

        // required-tags badges carry the display text as a `title` attr on
        // the outer span (e.g. `title="Teen And Up Audiences"`) — read that
        // rather than reverse-mapping the `rating-*`/`category-*` class
        // slug back into AO3's exact display string.
        let rating = row
            .select(&rating_sel)
            .next()
            .and_then(|e| e.value().attr("title"))
            .unwrap_or_default()
            .to_string();
        let category = row.select(&category_sel).next().and_then(|e| e.value().attr("title")).map(|s| s.to_string());
        let status = match row.select(&complete_sel).next().and_then(|e| e.value().attr("title")) {
            Some("Complete") => "complete",
            _ => "in_progress", // "Work in Progress", or badge missing entirely
        }
        .to_string();

        let warnings = join_tag_texts(&row, &warnings_sel);
        let tags = [
            join_tag_texts(&row, &relationship_sel),
            join_tag_texts(&row, &character_sel),
            join_tag_texts(&row, &freeform_sel),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let tags = if tags.is_empty() { None } else { Some(tags.join(", ")) };

        let summary = row.select(&summary_sel).next().map(|e| text_of(&e)).filter(|s| !s.is_empty());
        let language = row.select(&language_sel).next().map(|e| text_of(&e)).filter(|s| !s.is_empty());
        let word_count = row.select(&words_sel).next().map(|e| parse_word_count(&text_of(&e))).unwrap_or(0);
        let favorite_count = row.select(&bookmarks_sel).next().map(|e| parse_word_count(&text_of(&e))).unwrap_or(0);
        let chapters_stat = row.select(&chapters_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        let chapter_count = chapters_stat.split_once('/').map(|(n, _)| parse_word_count(n)).unwrap_or(1).max(1);

        out.push(Ao3StoryMetadata {
            source_story_id,
            title,
            author,
            fandom,
            rating,
            category,
            warnings,
            tags,
            language,
            status,
            word_count,
            favorite_count,
            chapter_count,
            summary,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tag_browse_url_omits_optional_filters_when_unset() {
        let url = build_tag_browse_url("Naruto", "kudos_count", 0, "any", 3);
        assert_eq!(
            url,
            "https://archiveofourown.org/works?tag_id=Naruto&work_search%5Bsort_column%5D=kudos_count&page=3"
        );
    }

    #[test]
    fn build_tag_browse_url_includes_word_count_and_status_when_set() {
        let url = build_tag_browse_url("Naruto (Anime & Manga)", "kudos_count", 100_000, "complete", 1);
        assert_eq!(
            url,
            "https://archiveofourown.org/works?tag_id=Naruto+%28Anime+%26+Manga%29&work_search%5Bsort_column%5D=kudos_count&page=1&work_search%5Bwords_from%5D=100000&work_search%5Bcomplete%5D=T"
        );

        let url = build_tag_browse_url("Naruto", "kudos_count", 0, "in_progress", 1);
        assert!(url.ends_with("&work_search%5Bcomplete%5D=F"));
    }

    #[test]
    fn tag_from_listing_url_recovers_the_tag_and_ignores_work_urls() {
        let listing_url = build_tag_browse_url("Naruto (Anime & Manga)", "kudos_count", 100_000, "complete", 1);
        assert_eq!(tag_from_listing_url(&listing_url).as_deref(), Some("Naruto (Anime & Manga)"));

        assert_eq!(tag_from_listing_url("https://archiveofourown.org/works/85954646"), None);
        assert_eq!(tag_from_listing_url("https://archiveofourown.org/works/85954646/chapters/227238471"), None);
    }

    #[test]
    fn work_id_from_url_does_not_false_positive_on_a_listing_url() {
        let listing_url = build_tag_browse_url("Naruto", "kudos_count", 0, "any", 1);
        assert!(work_id_from_url(&listing_url).is_err());
    }
}
