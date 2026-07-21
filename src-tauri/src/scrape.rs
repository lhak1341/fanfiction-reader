//! Generic HTML-scraping plumbing shared by `ffn.rs`/`ao3.rs` — architecture
//! review candidate 5, 2026-07-21. Deliberately narrow: no markup-shape
//! knowledge lives here, so this doesn't contradict CONTEXT.md's `FicSource`
//! entry (ffn.rs/ao3.rs's *parsing* stays fully separate, untouched by the
//! FicSource seam) — these two functions have no opinion on FFN vs AO3
//! markup shape, they're just "grab an element's trimmed text" and "parse a
//! digits-only count."

use scraper::ElementRef;

pub(crate) fn text_of(el: &ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

pub(crate) fn parse_int(s: &str) -> i64 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}
