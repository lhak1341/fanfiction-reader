//! Port of FanFicFare's `adapter_fanfictionnet.py` extraction logic (SPEC.md §4),
//! written against real markup captured from a live story page rather than
//! guessed from the Python source's BeautifulSoup calls.

use percent_encoding::percent_decode_str;
use scraper::{ElementRef, Html, Selector};

use crate::scrape::{parse_int, text_of};

#[derive(Debug, serde::Serialize)]
pub struct StoryMetadata {
    pub ffn_story_id: i64,
    pub title: String,
    pub author: String,
    pub fandom: String,
    pub rating: String,
    pub genre: Option<String>,
    pub language: Option<String>,
    /// Character/pairing segment(s) FFN shows after genre in the metadata
    /// line (e.g. "Naruto U." or, for pairings, "[Naruto U., Hinata H.]"),
    /// joined verbatim as one string if there are several. FFN's own term
    /// for these is "characters," but the app treats them as free-text tags.
    pub tags: Option<String>,
    pub status: String, // "complete" | "in_progress"
    pub word_count: i64,
    pub favorite_count: i64,
    pub chapter_count: i64,
    pub summary: Option<String>,
    /// Raw text from the metadata line's "Updated:" field, verbatim as FFN
    /// renders it — not reparsed into a real date since the exact format
    /// (relative vs. absolute, and whether it changes near the cutover) was
    /// never confirmed against live markup. `None` for one-shots, which
    /// don't have an Updated field at all (only Published).
    pub updated_at: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ChapterRef {
    pub chapter_num: i64,
    pub title: String,
}

/// Pulls the value out of a "Label: value - ..." segment, e.g. `"76,552"`
/// out of `"... Words: 76,552 - Reviews: ..."`.
fn extract_labeled(haystack: &str, label: &str) -> Option<String> {
    let start = haystack.find(label)? + label.len();
    let rest = &haystack[start..];
    let end = rest.find(" - ").unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

/// FFN story URLs look like `/s/8067258/1/Worth-Dying-For` — the story id is
/// the second path segment, the chapter number the third. Mirrors
/// FanFicFare's `set_story_idurl`.
///
/// `Ok(None)` means the URL isn't this shape at all (leading segment isn't
/// `s`) — architecture review candidate 1 (2026-07-21): this used to be
/// checked per-field by two separate functions that each blindly skipped
/// segment 0, so a URL like `/u/12345/someone` would false-match as story id
/// 12345. Checking the shape once, here, for both ids together means they
/// can't disagree about it.
pub fn ffn_locator_from_url(url: &str) -> Result<Option<(i64, i64)>, String> {
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    let mut segs = parsed.path_segments().ok_or("URL had no path")?;
    if segs.next() != Some("s") {
        return Ok(None);
    }
    let Some(story_id) = segs.next() else { return Ok(None) };
    let Some(chapter_num) = segs.next() else { return Ok(None) };
    let story_id: i64 = story_id.parse().map_err(|_| format!("story id segment {story_id:?} was not numeric"))?;
    let chapter_num: i64 =
        chapter_num.parse().map_err(|_| format!("chapter segment {chapter_num:?} was not numeric"))?;
    Ok(Some((story_id, chapter_num)))
}

pub fn parse_story_page(html: &str, ffn_story_id: i64) -> Result<(StoryMetadata, Vec<ChapterRef>), String> {
    let doc = Html::parse_document(html);

    let profile_top_sel = Selector::parse("#profile_top").unwrap();
    let profile_el = doc
        .select(&profile_top_sel)
        .next()
        .ok_or("could not find #profile_top — page may not be a story page")?;

    let title_sel = Selector::parse("b.xcontrast_txt").unwrap();
    let title = profile_el
        .select(&title_sel)
        .next()
        .map(|e| text_of(&e))
        .ok_or("could not find story title")?;

    let author_sel = Selector::parse(r#"a.xcontrast_txt[href^="/u/"]"#).unwrap();
    let author = profile_el
        .select(&author_sel)
        .next()
        .map(|e| text_of(&e))
        .unwrap_or_default();

    // The summary is the only direct-child <div class="xcontrast_txt"> of
    // #profile_top — everything else at that level is a <span> or <b>.
    let summary_sel = Selector::parse("div.xcontrast_txt").unwrap();
    let summary = profile_el.select(&summary_sel).next().map(|e| text_of(&e));

    let meta_sel = Selector::parse("span.xgray").unwrap();
    let meta_text = profile_el
        .select(&meta_sel)
        .next()
        .map(|e| text_of(&e))
        .ok_or("could not find the Rated/Words/Favs metadata line")?;

    // Breadcrumb (#pre_story_links) ends with the fandom, e.g.
    // "Anime/Manga > Naruto". Crossovers render two fandoms here — not
    // handled specially for v1 (SPEC.md scope is single-fandom browsing).
    let fandom_sel = Selector::parse("#pre_story_links a").unwrap();
    let fandom = doc.select(&fandom_sel).last().map(|e| text_of(&e)).unwrap_or_default();

    let rating = extract_labeled(&meta_text, "Rated:")
        .and_then(|s| s.split_whitespace().last().map(|s| s.to_string()))
        .unwrap_or_default();

    let word_count = extract_labeled(&meta_text, "Words:").map(|s| parse_int(&s)).unwrap_or(0);
    let favorite_count = extract_labeled(&meta_text, "Favs:").map(|s| parse_int(&s)).unwrap_or(0);
    let status = if meta_text.contains("Complete") { "complete" } else { "in_progress" }.to_string();
    let updated_at = extract_labeled(&meta_text, "Updated:").filter(|s| !s.is_empty());

    // Language/genre/tags: FFN doesn't label these, so read them
    // positionally between "Rated: <value>" and the first keyed label
    // (Chapters:/Words:). Anything past genre is character/pairing tag
    // segments (e.g. "Naruto U.", or "[Naruto U., Hinata H.]" for a
    // pairing) — joined verbatim rather than parsed further.
    let after_rated = meta_text.splitn(2, "Rated:").nth(1).unwrap_or("");
    let before_keyed_end = ["Chapters:", "Words:"]
        .iter()
        .filter_map(|label| after_rated.find(label))
        .min()
        .unwrap_or(after_rated.len());
    let segments: Vec<&str> = after_rated[..before_keyed_end]
        .split(" - ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    // segments[0] is the rating value itself (e.g. "Fiction  T").
    let language = segments.get(1).map(|s| s.to_string());
    let genre = segments.get(2).map(|s| s.to_string());
    let tags = if segments.len() > 3 {
        Some(segments[3..].join(", "))
    } else {
        None
    };

    let chapter_sel = Selector::parse(r#"select[name="chapter"] option"#).unwrap();
    let mut chapters: Vec<ChapterRef> = doc
        .select(&chapter_sel)
        .filter_map(|opt| {
            let num: i64 = opt.value().attr("value")?.parse().ok()?;
            let raw = text_of(&opt);
            let title = raw
                .split_once('.')
                .map(|(_, rest)| rest.trim().to_string())
                .unwrap_or(raw);
            Some(ChapterRef { chapter_num: num, title })
        })
        .collect();
    chapters.sort_by_key(|c| c.chapter_num);
    chapters.dedup_by_key(|c| c.chapter_num);
    if chapters.is_empty() {
        // One-shots don't render a chapter dropdown at all.
        chapters.push(ChapterRef { chapter_num: 1, title: title.clone() });
    }

    let metadata = StoryMetadata {
        ffn_story_id,
        title,
        author,
        fandom,
        rating,
        genre,
        language,
        tags,
        status,
        word_count,
        favorite_count,
        chapter_count: chapters.len() as i64,
        summary,
        updated_at,
    };

    Ok((metadata, chapters))
}

/// Builds an FFN browse-listing URL with server-side sort/filter, so page 1
/// is already the most-relevant results rather than an arbitrary slice —
/// see wayfinder decision trail / SPEC.md §6 "discovery". Confirmed against
/// a real filtered FFN URL during development:
/// `srt=4` = sort by Favorites (FFN has no native sort-by-word-count, so
/// imports always use this — word-count/updated sort is a local re-sort
/// after import instead). `len` = minimum word count in thousands (omitted
/// for "any length"). `s=2` = Complete (the only status value confirmed;
/// "in_progress"/"any" are left unfiltered server-side and narrowed locally
/// after import instead of risking a wrong guessed code). `r=10` = "no
/// rating filter" (show every rating including M) — without it, FFN's
/// default excludes Mature-rated stories from browse listings, which was
/// silently dropping some of the highest-favorited fics in a fandom out of
/// the imported pool entirely (confirmed by diffing a real 100K+ Pokémon
/// query with and without it — several top-favorited results, e.g. "Borne
/// of Caution", only appear with `r=10` present). Always sent, same as
/// `srt=4`.
pub fn build_browse_url(category: &str, fandom: &str, min_word_count: i64, status: &str, page: i64) -> String {
    let fandom_slug = fandom.replace(' ', "-");
    let mut url = format!("https://www.fanfiction.net/{category}/{fandom_slug}/?srt=4&r=10");
    if min_word_count > 0 {
        url.push_str(&format!("&len={}", min_word_count / 1000));
    }
    if status == "complete" {
        url.push_str("&s=2");
    }
    url.push_str(&format!("&p={page}"));
    url
}

/// Pulls whatever trails the "Published: <date>" segment on a browse-page
/// metadata line — that's where character/pairing tags sit there, unlike
/// the story page where they sit right after genre. `None` if a story has
/// no tags listed (some don't). A completed story appends a trailing
/// "- Complete" marker after the tags on this same line (in-progress rows
/// have no such marker) — stripped here so it doesn't leak into `tags`
/// (caught by `parse_browse_page_extracts_real_markup`, architecture
/// review candidate 5, 2026-07-19).
fn extract_trailing_tags(meta_text: &str) -> Option<String> {
    let after_published = meta_text.split("Published:").nth(1)?;
    let mut tags = after_published.splitn(2, " - ").nth(1)?.trim();
    tags = tags.strip_suffix(" - Complete").unwrap_or(tags).trim();
    if tags == "Complete" {
        tags = "";
    }
    if tags.is_empty() {
        None
    } else {
        Some(tags.to_string())
    }
}

/// FFN's abbreviated counts on the category index page: "444K" -> 444000,
/// "85.3K" -> 85300, "523" -> 523 (no suffix under 1,000).
fn parse_ffn_count(raw: &str) -> i64 {
    let raw = raw.trim();
    if let Some(digits) = raw.strip_suffix('K') {
        let value: f64 = digits.parse().unwrap_or(0.0);
        (value * 1000.0).round() as i64
    } else {
        raw.parse().unwrap_or(0)
    }
}

/// Parses an FFN category index page (e.g. `https://www.fanfiction.net/anime/`)
/// into `(fandom, ffn_count)` pairs, filtered to `min_count` and up — the
/// index is already sorted by count descending and can run into the
/// thousands, most of it a long tail not worth surfacing (locked during
/// discovery design: only fandoms with 1,000+ fics matter for browsing).
pub fn parse_fandom_index(html: &str, min_count: i64) -> Vec<(String, i64)> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("#list_output a").unwrap();

    let mut out = Vec::new();
    for a in doc.select(&row_sel) {
        let fandom = a.value().attr("title").map(|s| s.to_string()).unwrap_or_else(|| text_of(&a));
        // The count lives in a sibling <span class="gray">, not inside the <a>.
        let Some(count_text) = a
            .next_siblings()
            .find_map(|node| ElementRef::wrap(node).filter(|el| el.value().attr("class") == Some("gray")))
            .map(|el| text_of(&el))
        else {
            continue;
        };
        let count = parse_ffn_count(count_text.trim_start_matches('(').trim_end_matches(')'));
        if count < min_count {
            // Sorted descending — everything after this point is smaller too.
            break;
        }
        out.push((fandom, count));
    }
    out
}

/// Category-index URLs look like `/anime/` — exactly one non-empty path
/// segment, no fandom. Distinguishes an index-page fetch from a
/// browse-listing fetch (`/anime/Naruto/`, two segments) when dispatching
/// on URL shape alone.
pub fn category_from_index_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let mut segs = parsed.path_segments()?.filter(|s| !s.is_empty());
    let category = segs.next()?;
    if segs.next().is_some() {
        return None; // a second segment means this is a browse-listing URL, not an index.
    }
    Some(category.to_string())
}

/// Browse/category URLs look like `/anime/Naruto/` — fandom is the second
/// path segment. Unlike story pages, browse pages carry no `#pre_story_links`
/// breadcrumb naming the fandom anywhere in the markup, so the URL is the
/// only source for it. Dashes are FFN's word-separator convention in these
/// slugs for multi-word fandoms (e.g. `/book/Harry-Potter/`).
/// Returns `(category, fandom, page)` — `page` defaults to 1 if the URL has
/// no `p=` query param (FFN treats an absent page as page 1).
pub fn parse_browse_url(url: &str) -> Option<(String, String, i64)> {
    let parsed = url::Url::parse(url).ok()?;
    let mut segs = parsed.path_segments()?;
    let category = segs.next()?.to_string();
    let raw_fandom = segs.next()?;
    if category.is_empty() || raw_fandom.is_empty() {
        return None;
    }
    let fandom = percent_decode_str(raw_fandom).decode_utf8_lossy().replace('-', " ");
    let page = parsed
        .query_pairs()
        .find(|(k, _)| k == "p")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(1);
    Some((category, fandom, page))
}

/// Parses an FFN browse/category listing page (e.g.
/// `https://www.fanfiction.net/anime/Naruto/`) into one [`StoryMetadata`]
/// per story shown. Each entry's `chapter_count` comes from the listing's
/// own "Chapters: N" field (no per-chapter titles — those are only known
/// once a story's own page has been fetched, same as any newly-discovered
/// story). `fandom` is passed in (from [`fandom_from_browse_url`]) rather
/// than scraped — the listing page has no element naming it anywhere.
pub fn parse_browse_page(html: &str, fandom: &str) -> Result<Vec<StoryMetadata>, String> {
    let doc = Html::parse_document(html);

    let row_sel = Selector::parse("div.z-list").unwrap();
    let title_sel = Selector::parse("a.stitle").unwrap();
    let author_sel = Selector::parse(r#"a[href^="/u/"]"#).unwrap();
    let meta_sel = Selector::parse("div.z-padtop2").unwrap();
    let summary_sel = Selector::parse("div.z-indent").unwrap();

    let mut out = Vec::new();

    for row in doc.select(&row_sel) {
        let Some(title_el) = row.select(&title_sel).next() else { continue };
        let Some(ffn_story_id) = title_el
            .value()
            .attr("href")
            .and_then(|href| href.split('/').nth(2))
            .and_then(|seg| seg.parse::<i64>().ok())
        else {
            continue;
        };
        let title = text_of(&title_el);
        let author = row.select(&author_sel).next().map(|e| text_of(&e)).unwrap_or_default();

        // The summary div's own direct text nodes, excluding its nested
        // metadata div's text (that's parsed separately via meta_sel).
        let summary = row.select(&summary_sel).next().map(|el| {
            el.children()
                .filter_map(|node| node.value().as_text().map(|t| t.to_string()))
                .collect::<String>()
                .trim()
                .to_string()
        });
        let summary = summary.filter(|s| !s.is_empty());

        let meta_text = row.select(&meta_sel).next().map(|e| text_of(&e)).unwrap_or_default();

        let rating = extract_labeled(&meta_text, "Rated:")
            .and_then(|s| s.split_whitespace().last().map(|s| s.to_string()))
            .unwrap_or_default();
        let word_count = extract_labeled(&meta_text, "Words:").map(|s| parse_int(&s)).unwrap_or(0);
        let favorite_count = extract_labeled(&meta_text, "Favs:").map(|s| parse_int(&s)).unwrap_or(0);
        let chapter_count = extract_labeled(&meta_text, "Chapters:").map(|s| parse_int(&s)).unwrap_or(1);
        let status = if meta_text.contains("Complete") { "complete" } else { "in_progress" }.to_string();
        let updated_at = extract_labeled(&meta_text, "Updated:").filter(|s| !s.is_empty());

        let after_rated = meta_text.splitn(2, "Rated:").nth(1).unwrap_or("");
        let before_keyed_end = ["Chapters:", "Words:"]
            .iter()
            .filter_map(|label| after_rated.find(label))
            .min()
            .unwrap_or(after_rated.len());
        let segments: Vec<&str> = after_rated[..before_keyed_end]
            .split(" - ")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let language = segments.get(1).map(|s| s.to_string());
        let genre = segments.get(2).map(|s| s.to_string());
        let tags = extract_trailing_tags(&meta_text);

        out.push(StoryMetadata {
            ffn_story_id,
            title,
            author,
            fandom: fandom.to_string(),
            rating,
            genre,
            language,
            tags,
            status,
            word_count,
            favorite_count,
            chapter_count,
            summary,
            updated_at,
        });
    }

    Ok(out)
}

/// Extracts a single chapter's body. `#storytext`'s inner HTML (not text) is
/// kept so paragraph breaks, italics, etc. survive into the reader.
pub fn parse_chapter_content(html: &str) -> Result<String, String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("#storytext").unwrap();
    doc.select(&sel)
        .next()
        .map(|el| el.inner_html())
        .ok_or_else(|| "could not find #storytext".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real markup captured live (2026-07-19) — a 144-chapter, in-progress
    // story page (viewing chapter 34) — see architecture review candidate 5.
    const STORY_PAGE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_story_page.html"));

    #[test]
    fn parse_story_page_extracts_real_markup() {
        let (meta, chapters) = parse_story_page(STORY_PAGE, 13_493_099).unwrap();

        assert_eq!(meta.ffn_story_id, 13_493_099);
        assert_eq!(meta.title, "Welcome to the (insane) world of Pokémon");
        assert_eq!(meta.author, "CaptainMoonshine");
        assert_eq!(meta.fandom, "Pokémon");
        assert_eq!(meta.rating, "M");
        assert_eq!(meta.genre.as_deref(), Some("Adventure/Mystery"));
        assert_eq!(meta.language.as_deref(), Some("English"));
        assert_eq!(meta.tags.as_deref(), Some("Riolu, Kirlia, OC"));
        assert_eq!(meta.status, "in_progress");
        assert_eq!(meta.word_count, 510_859);
        assert_eq!(meta.favorite_count, 7_486);
        assert_eq!(meta.chapter_count, 144);
        assert!(meta.updated_at.is_some());

        // The dropdown is rendered twice on a real page (top + bottom nav) —
        // this is the sort_by_key/dedup_by_key pair's actual job, not just
        // defensive code.
        assert_eq!(chapters.len(), 144);
        assert_eq!(chapters[0].chapter_num, 1);
        assert_eq!(chapters[0].title, "Summoned");
        assert_eq!(chapters[143].chapter_num, 144);
        assert_eq!(chapters[143].title, "Perfectly Flawless");
    }

    #[test]
    fn parse_chapter_content_extracts_storytext() {
        let content = parse_chapter_content(STORY_PAGE).unwrap();
        assert!(content.len() > 500, "expected real chapter prose, got {} bytes", content.len());
    }

    // Real markup captured live (2026-07-19) via the app's own webview —
    // Fullmetal Alchemist browse listing, and the Books category index.
    const BROWSE_LISTING: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_browse_listing.html"));
    const CATEGORY_INDEX: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffn_category_index.html"));

    #[test]
    fn parse_browse_page_extracts_real_markup() {
        let stories = parse_browse_page(BROWSE_LISTING, "Fullmetal Alchemist").unwrap();
        assert_eq!(stories.len(), 25);

        let first = &stories[0];
        assert_eq!(first.ffn_story_id, 2_783_898);
        assert_eq!(first.title, "Full Circle");
        assert_eq!(first.author, "Nike Femme");
        assert_eq!(first.fandom, "Fullmetal Alchemist");
        assert_eq!(first.rating, "T");
        assert_eq!(first.genre.as_deref(), Some("Adventure/Drama"));
        assert_eq!(first.language.as_deref(), Some("English"));
        assert_eq!(first.status, "complete");
        assert_eq!(first.word_count, 144_252);
        assert_eq!(first.favorite_count, 3_019);
        assert_eq!(first.chapter_count, 36);
        // Regression case for the trailing "- Complete" marker bug found
        // while writing this test — see extract_trailing_tags.
        assert_eq!(first.tags.as_deref(), Some("Edward E., Roy M."));
    }

    #[test]
    fn parse_fandom_index_extracts_and_filters_real_markup() {
        let entries = parse_fandom_index(CATEGORY_INDEX, 1000);
        // Sorted descending by count; page has 2,616 fandoms total but only
        // those with >= 1,000 fics are surfaced (locked scope, see the
        // parse_fandom_index doc comment).
        assert_eq!(entries.len(), 71);
        assert_eq!(entries[0], ("Harry Potter".to_string(), 852_000));
        assert_eq!(entries[1], ("Twilight".to_string(), 223_000));
        assert_eq!(entries.last().unwrap(), &("Keeper of the Lost Cities".to_string(), 1_000));
    }

    #[test]
    fn ffn_locator_from_url_matches_a_real_chapter_url() {
        let locator = ffn_locator_from_url("https://www.fanfiction.net/s/8067258/1/Worth-Dying-For").unwrap();
        assert_eq!(locator, Some((8_067_258, 1)));
    }

    #[test]
    fn ffn_locator_from_url_no_match_for_a_profile_url() {
        // architecture review candidate 1: this used to false-match as story
        // id 12345 because the leading segment was never checked.
        let locator = ffn_locator_from_url("https://www.fanfiction.net/u/12345/someone").unwrap();
        assert_eq!(locator, None);
    }

    #[test]
    fn ffn_locator_from_url_malformed_for_non_numeric_story_id() {
        let err = ffn_locator_from_url("https://www.fanfiction.net/s/not-a-number/1/Title").unwrap_err();
        assert!(err.contains("story id segment"), "unexpected error: {err}");
    }

    #[test]
    fn ffn_locator_from_url_malformed_for_non_numeric_chapter_num() {
        let err = ffn_locator_from_url("https://www.fanfiction.net/s/8067258/not-a-number/Title").unwrap_err();
        assert!(err.contains("chapter segment"), "unexpected error: {err}");
    }
}
