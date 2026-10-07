//! Fetch session: the one in-flight FFN webview fetch and its lifecycle —
//! start, supersede, deliver, time out. It hands the rendered page to the
//! command waiting on it and never persists: parsing and the DB write happen
//! in that waiting command (`fetch.rs`), the same shape `fetch_ao3_page`
//! already has. AO3's direct HTTP fetch does not use it.
//!
//! Replaces the old `FetchLock` + `emit_*` event fan-out, where the trigger
//! command returned at once and the result came back as one of six events
//! the frontend matched by content. That left a waiter hanging forever
//! whenever no event matched it (a navigation that never reported, an error
//! on a URL that classified as nothing), and stashed the in-flight fetch by
//! plain string equality between the requested URL and the webview's observed
//! `href` — which FFN's slug redirect and non-ASCII fandoms break.
//!
//! Deliberately free of `AppHandle`/`tauri::State`/`db::Db`: the webview
//! navigation and the persist step stay in `fetch.rs`, so every transition
//! here is unit-testable.
//!
//! "Newest wins" is unchanged: `start` rejects the fetch it replaces with
//! `"interrupted by a newer fetch"`. Whether a foreground fetch should
//! outrank a background one is a separate question this module does not
//! decide.

use crate::dispatch::{classify_page, PageShape};
use crate::source::{self, FfnListingLocator};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

/// How long `wait` lets the webview take to report before giving up. The
/// page's own init script reports within ~4 s of `DOMContentLoaded`
/// (`fetch_page_init.js`), so this only fires when it never ran at all — a
/// failed navigation, or `window.__TAURI__` missing.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

pub const INTERRUPTED: &str = "interrupted by a newer fetch";

/// What a fetch is *for*, compared between the requested URL and the URL the
/// webview ended up on. Two URLs that name the same chapter, listing page or
/// category index are the same target even when the strings differ (FFN adds
/// the story slug on redirect; `url::Url` percent-encodes a non-ASCII
/// fandom). A URL that classifies as nothing falls back to exact string
/// equality — only the debug harness fetches those.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetKey {
    FandomIndex(String),
    Chapter { story_id: i64, chapter_num: i64 },
    Listing { category: String, fandom: String, min_word_count: i64, page: i64 },
    Raw(String),
}

impl TargetKey {
    fn of(url: &str) -> Self {
        match classify_page::<source::Ffn>(url) {
            Ok(PageShape::FandomIndex(category)) => TargetKey::FandomIndex(category),
            Ok(PageShape::Chapter(l)) => TargetKey::Chapter { story_id: l.story_id, chapter_num: l.chapter_num },
            Ok(PageShape::Listing(l)) => TargetKey::Listing {
                category: l.category,
                fandom: l.fandom,
                min_word_count: l.min_word_count,
                page: l.page,
            },
            Ok(PageShape::Unknown) | Err(_) => TargetKey::Raw(url.to_string()),
        }
    }
}

/// The rendered page the webview reported, plus the listing locator the
/// fetch was started with (stashed as-requested because `build_browse_url`'s
/// encoding is lossy for `status` — see `FfnListingLocator`).
#[derive(Debug)]
pub struct FetchedPage {
    pub html: String,
    /// The webview's own `window.location.href` — post-redirect, so it can
    /// carry a slug the requested URL didn't.
    pub href: String,
    pub title: String,
    pub listing_locator: Option<FfnListingLocator>,
}

struct Pending {
    id: u64,
    key: TargetKey,
    listing_locator: Option<FfnListingLocator>,
    tx: oneshot::Sender<Result<FetchedPage, String>>,
}

#[derive(Default)]
struct Slot {
    next_id: u64,
    pending: Option<Pending>,
}

/// Handle to one started fetch: pass it to `FetchSession::wait`, or to
/// `abort` if navigating the webview failed.
pub struct Fetch {
    id: u64,
    rx: oneshot::Receiver<Result<FetchedPage, String>>,
}

pub struct FetchSession {
    slot: Mutex<Slot>,
    /// Serializes building/navigating the single reused `fetch-page` window:
    /// two concurrent `fetch_page` calls could otherwise both see no window
    /// and both try to build one under the same label.
    window_gate: tokio::sync::Mutex<()>,
}

impl Default for FetchSession {
    fn default() -> Self {
        Self::new()
    }
}

impl FetchSession {
    pub fn new() -> Self {
        Self { slot: Mutex::new(Slot::default()), window_gate: tokio::sync::Mutex::new(()) }
    }

    /// Held by `fetch_page` while it builds or navigates the window.
    pub async fn lock_window(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.window_gate.lock().await
    }

    /// Registers a new fetch for `url`, rejecting whichever fetch was still
    /// in flight — a `navigate()` over a loading page kills that page's init
    /// script, so its report would never come.
    pub fn start(&self, url: &str, listing_locator: Option<FfnListingLocator>) -> Fetch {
        let (tx, rx) = oneshot::channel();
        let mut slot = self.slot.lock().expect("fetch session slot poisoned");
        let id = slot.next_id;
        slot.next_id += 1;
        let previous = slot.pending.replace(Pending { id, key: TargetKey::of(url), listing_locator, tx });
        drop(slot);
        if let Some(previous) = previous {
            let _ = previous.tx.send(Err(INTERRUPTED.to_string()));
        }
        Fetch { id, rx }
    }

    /// The webview reported a page. Delivered to the pending fetch only if
    /// `href` names the same target it was started for; anything else (a
    /// stale report from a superseded or timed-out page, a navigation to some
    /// other URL) is dropped. Returns whether it was delivered.
    pub fn deliver(&self, href: &str, html: String, title: String) -> bool {
        let mut slot = self.slot.lock().expect("fetch session slot poisoned");
        if slot.pending.as_ref().map(|p| &p.key) != Some(&TargetKey::of(href)) {
            return false;
        }
        let pending = slot.pending.take().expect("checked above");
        drop(slot);
        let page = FetchedPage { html, href: href.to_string(), title, listing_locator: pending.listing_locator };
        let _ = pending.tx.send(Ok(page));
        true
    }

    /// Fails `fetch` with `message` if it is still the pending one — for a
    /// webview that could not even be navigated.
    pub fn abort(&self, fetch: &Fetch, message: String) {
        if let Some(pending) = self.take_if_current(fetch.id) {
            let _ = pending.tx.send(Err(message));
        }
    }

    /// Resolves with the delivered page, `INTERRUPTED` if a newer `start`
    /// replaced this fetch, or a timeout error — in which case the slot is
    /// freed, so a report that arrives late is dropped by `deliver`.
    pub async fn wait(&self, fetch: Fetch, timeout: Duration) -> Result<FetchedPage, String> {
        let Fetch { id, rx } = fetch;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("fetch abandoned before the page reported".to_string()),
            Err(_) => {
                self.take_if_current(id);
                Err(format!(
                    "fetch timed out after {}s (page never reported — navigation failure or the init script did not run?)",
                    timeout.as_secs_f32()
                ))
            }
        }
    }

    fn take_if_current(&self, id: u64) -> Option<Pending> {
        let mut slot = self.slot.lock().expect("fetch session slot poisoned");
        if slot.pending.as_ref().is_some_and(|p| p.id == id) {
            slot.pending.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffn;

    const SLUGLESS: &str = "https://www.fanfiction.net/s/13493099/34/";
    const SLUGGED: &str = "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";
    const SHORT: Duration = Duration::from_millis(30);

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(f)
    }

    fn locator(fandom: &str, status: &str) -> FfnListingLocator {
        FfnListingLocator {
            category: "anime".to_string(),
            fandom: fandom.to_string(),
            min_word_count: 0,
            status: status.to_string(),
            page: 1,
        }
    }

    #[test]
    fn delivers_a_redirected_href_that_names_the_same_chapter() {
        let session = FetchSession::new();
        let fetch = session.start(SLUGLESS, None);

        assert!(session.deliver(SLUGGED, "<html/>".into(), "t".into()));

        let page = block_on(session.wait(fetch, SHORT)).unwrap();
        assert_eq!(page.href, SLUGGED);
        assert_eq!(page.html, "<html/>");
    }

    #[test]
    fn delivers_a_non_ascii_fandom_listing_and_keeps_the_stashed_locator() {
        let session = FetchSession::new();
        let requested = ffn::build_browse_url("anime", "Pokémon", 0, "in_progress", 1);
        // What the webview reports back: the same path, percent-encoded.
        let observed = requested.replace('é', "%C3%A9");
        assert_ne!(requested, observed);
        let fetch = session.start(&requested, Some(locator("Pokémon", "in_progress")));

        assert!(session.deliver(&observed, String::new(), String::new()));

        let page = block_on(session.wait(fetch, SHORT)).unwrap();
        // The lossy decoded status ("any") must not replace the stashed one.
        assert_eq!(page.listing_locator.unwrap().status, "in_progress");
    }

    #[test]
    fn ignores_a_report_for_a_different_target() {
        let session = FetchSession::new();
        let fetch = session.start(SLUGLESS, None);

        assert!(!session.deliver("https://www.fanfiction.net/s/13493099/35/Other", String::new(), String::new()));

        // Still pending: it times out rather than being resolved by the stray report.
        let err = block_on(session.wait(fetch, SHORT)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn a_newer_start_rejects_the_older_fetch_and_leaves_the_new_one_pending() {
        let session = FetchSession::new();
        let older = session.start(SLUGLESS, None);
        let newer = session.start("https://www.fanfiction.net/s/1/1/", None);

        assert_eq!(block_on(session.wait(older, SHORT)).unwrap_err(), INTERRUPTED);

        assert!(session.deliver("https://www.fanfiction.net/s/1/1/Slug", String::new(), String::new()));
        assert!(block_on(session.wait(newer, SHORT)).is_ok());
    }

    #[test]
    fn a_stale_report_for_the_superseded_target_is_dropped() {
        let session = FetchSession::new();
        let _older = session.start(SLUGLESS, None);
        let _newer = session.start("https://www.fanfiction.net/s/1/1/", None);

        assert!(!session.deliver(SLUGGED, String::new(), String::new()));
    }

    #[test]
    fn timeout_rejects_and_frees_the_slot_so_a_late_report_is_dropped() {
        let session = FetchSession::new();
        let fetch = session.start(SLUGLESS, None);

        let err = block_on(session.wait(fetch, SHORT)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");

        assert!(!session.deliver(SLUGGED, String::new(), String::new()));
    }

    #[test]
    fn a_timed_out_fetch_does_not_clear_a_newer_one() {
        let session = FetchSession::new();
        let older = session.start(SLUGLESS, None);
        let _newer = session.start("https://www.fanfiction.net/s/1/1/", None);

        // `older` was already interrupted; its own timeout path must not
        // take the newer fetch's slot.
        let _ = block_on(session.wait(older, SHORT));

        assert!(session.deliver("https://www.fanfiction.net/s/1/1/", String::new(), String::new()));
    }

    #[test]
    fn abort_fails_the_fetch_and_frees_the_slot() {
        let session = FetchSession::new();
        let fetch = session.start(SLUGLESS, None);

        session.abort(&fetch, "navigate failed".to_string());

        assert_eq!(block_on(session.wait(fetch, SHORT)).unwrap_err(), "navigate failed");
        assert!(!session.deliver(SLUGGED, String::new(), String::new()));
    }

    #[test]
    fn abort_of_a_superseded_fetch_leaves_the_newer_one_pending() {
        let session = FetchSession::new();
        let older = session.start(SLUGLESS, None);
        let newer = session.start("https://www.fanfiction.net/s/1/1/", None);

        session.abort(&older, "navigate failed".to_string());

        assert!(session.deliver("https://www.fanfiction.net/s/1/1/", String::new(), String::new()));
        assert!(block_on(session.wait(newer, SHORT)).is_ok());
    }

    #[test]
    fn an_unclassifiable_url_matches_only_by_exact_string() {
        let session = FetchSession::new();
        let fetch = session.start("https://example.com/?a=1", None);

        assert!(!session.deliver("https://example.com/?a=2", String::new(), String::new()));
        assert!(session.deliver("https://example.com/?a=1", String::new(), String::new()));
        assert!(block_on(session.wait(fetch, SHORT)).is_ok());
    }

    #[test]
    fn a_category_index_report_matches_by_category() {
        let session = FetchSession::new();
        let fetch = session.start("https://www.fanfiction.net/anime/", None);

        assert!(session.deliver("https://www.fanfiction.net/anime/", String::new(), String::new()));
        assert!(block_on(session.wait(fetch, SHORT)).is_ok());
    }
}
