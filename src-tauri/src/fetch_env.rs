//! The fetch environment: everything `fetch.rs`'s orchestration needs from the
//! outside world, behind one seam (`FetchEnv`) — the FFN webview round-trip
//! (`navigate`; the page itself comes back through `FetchSession`), AO3's
//! direct HTTP call (`get`), and the best-effort debug dump
//! (`save_debug_page`). The real adapter is `TauriEnv` (the app's hidden
//! `fetch-page` window, `reqwest`, the app data dir); tests use `testing::FakeEnv`,
//! so the whole fetch sequence of either source — and AO3's retry policy —
//! runs without Tauri or a network.
//!
//! The AO3 retry loop (`get_with_retry`) lives *above* the seam: the adapter
//! only classifies what happened (`Ao3Response` / `GetError::Retryable` /
//! `GetError::Fatal`), so the policy is tested from scripted responses.

use crate::fetch_session::FETCH_TIMEOUT;
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::Manager;

/// What `fetch.rs` needs from the app. `ffn_wait` and `ao3_base_backoff` are
/// the two timing knobs a test shortens; the defaults are production's.
pub trait FetchEnv {
    /// Points the FFN webview at `url` (building it on the app's first fetch).
    fn navigate(&self, url: url::Url) -> Result<(), String>;
    /// One AO3 HTTP GET, following redirects. Only classifies the outcome —
    /// retrying is `get_with_retry`'s job.
    fn get(&self, url: &str) -> impl Future<Output = Result<Ao3Response, GetError>> + Send;
    /// Best-effort copy of a fetched page for inspection (`debug-last-fetch.html`).
    fn save_debug_page(&self, file_name: &str, html: &str);

    /// How long one FFN fetch waits for the webview to report a page.
    fn ffn_wait(&self) -> Duration {
        FETCH_TIMEOUT
    }
    /// How long one FFN fetch waits past a Cloudflare challenge page before
    /// loading the URL again (`fetch_ffn_attempt`).
    fn ffn_challenge_wait(&self) -> Duration {
        CHALLENGE_WAIT
    }
    /// First AO3 retry delay; each later one doubles it.
    fn ao3_base_backoff(&self) -> Duration {
        AO3_BASE_BACKOFF
    }
}

/// A completed AO3 request. `body` is only read for a success status.
#[derive(Debug, Clone)]
pub struct Ao3Response {
    pub status: reqwest::StatusCode,
    /// The post-redirect URL.
    pub final_url: String,
    pub body: String,
}

/// A request that never produced a status.
#[derive(Debug, Clone, PartialEq)]
pub enum GetError {
    /// A connection or timeout failure — worth another attempt.
    Retryable(String),
    /// Anything else (bad URL, unreadable body) — retrying won't help.
    Fatal(String),
}

// ---- AO3 fetch path (SPEC.md §8) ----
//
// Deliberately not a copy of `fetch_page`'s webview/window/event-callback
// machinery: that whole apparatus exists only to get past Cloudflare (§3),
// and a bare HTTP client *is* sufficient for AO3 — no webview, no cookies,
// no JS needed. So this fetches and persists in one direct async call
// rather than firing a window and waiting for a `report_fetch_result`
// round-trip.
//
// **Revises §8's "no bot-fingerprinting" finding**: re-tested live
// (2026-07-19, this session) with `reqwest`'s and `urllib`'s *default*
// User-Agent strings and got a real Cloudflare "managed challenge" (503
// wrapper page, "Shields are up!") on both a work page and a tag listing —
// not the flakiness §8 already documented, a genuine block. Setting a
// realistic desktop-browser User-Agent header made both fetch clean on the
// first try, confirmed with both `urllib` and `reqwest` directly. So AO3's
// Cloudflare *is* doing UA-based filtering, just not full TLS/JS
// fingerprinting the way FFN's does (§3) — a static UA header clears it,
// no webview/interactive-challenge machinery required.
//
// What AO3's origin *does* still need, confirmed live (§8), is
// retry-with-backoff: a handful of bare requests hit plain `TimeoutError`s
// and an `HTTP 525` (Cloudflare origin SSL error) before a retry succeeded,
// with no challenge page involved — ordinary origin flakiness, distinct
// from the UA-based block above. FFN's fetch loop never needed retries,
// only inter-request pacing (§3); AO3's needs the opposite.

pub const AO3_FETCH_MAX_ATTEMPTS: u32 = 4;
/// A challenge that passes redirects within a few seconds; one that has not
/// by now is treated as stuck (see `fetch_ffn_attempt`).
const CHALLENGE_WAIT: Duration = Duration::from_secs(8);
const AO3_BASE_BACKOFF: Duration = Duration::from_millis(500);
const AO3_FETCH_TIMEOUT_SECS: u64 = 20;

// A real desktop-browser UA — confirmed live (above) necessary to avoid a
// Cloudflare "managed challenge" 403 that reqwest's/urllib's own default UA
// strings both trigger.
pub(crate) const AO3_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/// Fetches a single AO3 URL, retrying on connection/timeout errors and 5xx
/// responses (the flakiness confirmed live in SPEC.md §8) with exponential
/// backoff (`ao3_base_backoff`, then double, ...). Does *not* retry on 4xx — a
/// bad tag/work id (or a Cloudflare challenge the UA header above didn't
/// clear) isn't going to fix itself. Returns the body alongside the final
/// post-redirect URL: AO3 redirects a bare `/works/{id}` to that work's actual
/// first chapter URL, and callers need the URL that was *actually* served to
/// know which chapter this HTML is.
pub async fn get_with_retry(env: &impl FetchEnv, url: &str) -> Result<(String, String), String> {
    let mut last_err = String::new();
    for attempt in 1..=AO3_FETCH_MAX_ATTEMPTS {
        match env.get(url).await {
            Ok(resp) if resp.status.is_server_error() => {
                last_err = format!("HTTP {}", resp.status);
            }
            Ok(resp) if !resp.status.is_success() => {
                return Err(format!(
                    "AO3 returned HTTP {} (not a server error, won't retry — possibly a Cloudflare challenge or a bad url)",
                    resp.status
                ));
            }
            Ok(resp) => return Ok((resp.body, resp.final_url)),
            Err(GetError::Retryable(e)) => last_err = e,
            Err(GetError::Fatal(e)) => return Err(e),
        }

        if attempt < AO3_FETCH_MAX_ATTEMPTS {
            let backoff = env.ao3_base_backoff() * 2u32.pow(attempt - 1);
            println!(
                "[ao3-fetch] attempt {attempt}/{AO3_FETCH_MAX_ATTEMPTS} failed ({last_err}), retrying in {}ms",
                backoff.as_millis()
            );
            tokio::time::sleep(backoff).await;
        }
    }
    Err(format!("AO3 fetch failed after {AO3_FETCH_MAX_ATTEMPTS} attempts: {last_err}"))
}

static AO3_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

/// The `reqwest` adapter for `FetchEnv::get`: one shared client (timeout and
/// browser User-Agent above), classifying connection/timeout failures as
/// retryable.
pub(crate) async fn reqwest_get(url: &str) -> Result<Ao3Response, GetError> {
    let client = AO3_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(AO3_FETCH_TIMEOUT_SECS))
                .user_agent(AO3_USER_AGENT)
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| GetError::Fatal(e.clone()))?;

    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            let final_url = resp.url().to_string();
            if !status.is_success() {
                return Ok(Ao3Response { status, final_url, body: String::new() });
            }
            resp.text()
                .await
                .map(|body| Ao3Response { status, final_url, body })
                .map_err(|e| GetError::Fatal(format!("failed reading response body: {e}")))
        }
        Err(e) if e.is_timeout() || e.is_connect() => Err(GetError::Retryable(e.to_string())),
        Err(e) => Err(GetError::Fatal(e.to_string())),
    }
}

/// Writes the raw page to `<app data dir>/<file_name>` so a failed parse or a
/// false-positive block check can still be inspected. Best-effort: a
/// missing data dir or a write error must never fail the fetch.
fn dump_last_fetch(app: &tauri::AppHandle, file_name: &str, html: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::write(dir.join(file_name), html);
    }
}

/// Points the single reused `fetch-page` window at `url`, building it on the
/// app's first fetch. Tauri's own on_page_load(Finished) turned out
/// unreliable on some FFN pages (browse/category pages in particular) —
/// never fired even though the page visibly rendered correctly, likely
/// confused by a subframe (ad/tracking iframe) navigating independently.
/// Triggering off the page's own native `load` event instead, via an init
/// script that runs before the page's own scripts on every navigation,
/// tracks what's actually on screen rather than Tauri's navigation-delegate
/// state. The init script and `on_navigation` below apply to every
/// subsequent `navigate()` call on this same webview, not just the first load.
fn open_or_navigate(app: &tauri::AppHandle, parsed: url::Url) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("fetch-page") {
        println!("[fetch] navigating existing window to {parsed}");
        existing.navigate(parsed).map_err(|e| e.to_string())?;
        return Ok(());
    }

    println!("[fetch] opening window for {parsed}");

    tauri::WebviewWindowBuilder::new(app, "fetch-page", tauri::WebviewUrl::External(parsed))
        .title("Fetching… (watch for a Cloudflare challenge)")
        .inner_size(900.0, 700.0)
        // Hidden by default — the window flashing on screen for every
        // chapter fetch/prefetch was disruptive. Caveat: if fanfiction.net
        // ever serves an interactive Cloudflare challenge (rare once
        // cf_clearance is established), there's no visible window to solve
        // it in and the fetch will just time out. Temporarily flip this to
        // `true` to debug that case.
        .visible(false)
        .focused(false)
        .on_navigation(|url| {
            println!("[fetch] on_navigation: {url}");
            true
        })
        .initialization_script(include_str!("fetch_page_init.js"))
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// The production `FetchEnv`: the app's hidden `fetch-page` webview, the
/// shared `reqwest` client and the app data dir.
pub struct TauriEnv<'a>(pub &'a tauri::AppHandle);

impl FetchEnv for TauriEnv<'_> {
    fn navigate(&self, url: url::Url) -> Result<(), String> {
        open_or_navigate(self.0, url)
    }

    async fn get(&self, url: &str) -> Result<Ao3Response, GetError> {
        reqwest_get(url).await
    }

    fn save_debug_page(&self, file_name: &str, html: &str) {
        dump_last_fetch(self.0, file_name, html);
    }
}

/// A scriptable `FetchEnv` for tests: FFN reports delivered to a
/// `FetchSession` after `navigate` (what `fetch_page_init.js` does on each
/// DOMContentLoaded), and a queue of AO3 responses.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::fetch_session::FetchSession;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    pub(crate) const SLUGGED_URL: &str =
        "https://www.fanfiction.net/s/13493099/34/Welcome-to-the-insane-world-of-Pok-mon";

    /// One report the fake webview makes after `navigate`, `after_ms` after the
    /// previous one.
    pub(crate) struct Report {
        pub after_ms: u64,
        pub html: &'static str,
        pub title: &'static str,
    }

    pub(crate) struct FakeEnv {
        pub session: Arc<FetchSession>,
        pub reports: Vec<Report>,
        /// What every navigation after the first reports; `None` repeats `reports`.
        pub reload_reports: Option<Vec<Report>>,
        /// The URL the fake webview reports pages under.
        pub report_url: String,
        pub navigate_error: Option<String>,
        pub navigations: Mutex<Vec<String>>,
        pub dumps: Mutex<Vec<(String, String)>>,
        pub ao3_script: Mutex<VecDeque<Result<Ao3Response, GetError>>>,
        pub gets: Mutex<Vec<String>>,
        pub ffn_wait: Duration,
        pub ffn_challenge_wait: Duration,
        pub ao3_base_backoff: Duration,
    }

    impl FakeEnv {
        pub(crate) fn new(session: &Arc<FetchSession>, reports: Vec<Report>) -> Self {
            Self {
                session: Arc::clone(session),
                reports,
                reload_reports: None,
                report_url: SLUGGED_URL.to_string(),
                navigate_error: None,
                navigations: Mutex::new(vec![]),
                dumps: Mutex::new(vec![]),
                ao3_script: Mutex::new(VecDeque::new()),
                gets: Mutex::new(vec![]),
                ffn_wait: Duration::from_millis(400),
                ffn_challenge_wait: Duration::from_millis(150),
                ao3_base_backoff: Duration::from_millis(1),
            }
        }

        /// An environment with no FFN webview behaviour, only scripted AO3 responses.
        pub(crate) fn ao3(script: Vec<Result<Ao3Response, GetError>>) -> Self {
            let env = Self::new(&Arc::new(FetchSession::new()), vec![]);
            *env.ao3_script.lock().unwrap() = script.into();
            env
        }
    }

    pub(crate) fn ok(body: &str, final_url: &str) -> Result<Ao3Response, GetError> {
        Ok(Ao3Response { status: reqwest::StatusCode::OK, final_url: final_url.to_string(), body: body.to_string() })
    }

    pub(crate) fn status(code: u16) -> Result<Ao3Response, GetError> {
        Ok(Ao3Response {
            status: reqwest::StatusCode::from_u16(code).unwrap(),
            final_url: String::new(),
            body: String::new(),
        })
    }

    impl FetchEnv for FakeEnv {
        fn navigate(&self, url: url::Url) -> Result<(), String> {
            let first = {
                let mut navigations = self.navigations.lock().unwrap();
                navigations.push(url.to_string());
                navigations.len() == 1
            };
            if let Some(e) = &self.navigate_error {
                return Err(e.clone());
            }
            let session = Arc::clone(&self.session);
            let report_url = self.report_url.clone();
            let reports = match &self.reload_reports {
                Some(reload) if !first => reload,
                _ => &self.reports,
            };
            let script: Vec<(u64, &'static str, &'static str)> =
                reports.iter().map(|r| (r.after_ms, r.html, r.title)).collect();
            tokio::spawn(async move {
                for (after_ms, html, title) in script {
                    tokio::time::sleep(Duration::from_millis(after_ms)).await;
                    session.deliver(&report_url, html.to_string(), title.to_string());
                }
            });
            Ok(())
        }

        async fn get(&self, url: &str) -> Result<Ao3Response, GetError> {
            self.gets.lock().unwrap().push(url.to_string());
            self.ao3_script
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(GetError::Fatal("no scripted AO3 response left".to_string())))
        }

        fn save_debug_page(&self, file_name: &str, html: &str) {
            self.dumps.lock().unwrap().push((file_name.to_string(), html.to_string()));
        }

        fn ffn_wait(&self) -> Duration {
            self.ffn_wait
        }

        fn ffn_challenge_wait(&self) -> Duration {
            self.ffn_challenge_wait
        }

        fn ao3_base_backoff(&self) -> Duration {
            self.ao3_base_backoff
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn block_on<F: Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(f)
    }

    /// `reqwest_get` opens real sockets, so its runtime needs IO enabled.
    fn block_on_io<F: Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn retryable(msg: &str) -> Result<Ao3Response, GetError> {
        Err(GetError::Retryable(msg.to_string()))
    }

    // ---- get_with_retry ----

    #[test]
    fn a_success_returns_the_body_and_the_post_redirect_url() {
        let env = FakeEnv::ao3(vec![ok("<html>work</html>", "https://archiveofourown.org/works/1/chapters/2")]);

        let (html, final_url) = block_on(get_with_retry(&env, "https://archiveofourown.org/works/1")).unwrap();

        assert_eq!(html, "<html>work</html>");
        assert_eq!(final_url, "https://archiveofourown.org/works/1/chapters/2");
        assert_eq!(*env.gets.lock().unwrap(), vec!["https://archiveofourown.org/works/1".to_string()]);
    }

    #[test]
    fn a_server_error_is_retried_until_it_succeeds() {
        let env = FakeEnv::ao3(vec![status(525), status(503), ok("body", "u")]);

        let (html, _) = block_on(get_with_retry(&env, "https://x/")).unwrap();

        assert_eq!(html, "body");
        assert_eq!(env.gets.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_retryable_transport_error_is_retried() {
        let env = FakeEnv::ao3(vec![retryable("timed out"), ok("body", "u")]);

        assert!(block_on(get_with_retry(&env, "https://x/")).is_ok());
        assert_eq!(env.gets.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_client_error_fails_at_once_without_retrying() {
        let env = FakeEnv::ao3(vec![status(404), ok("never reached", "u")]);

        let err = block_on(get_with_retry(&env, "https://x/")).unwrap_err();

        assert!(err.contains("404") && err.contains("won't retry"), "{err}");
        assert_eq!(env.gets.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_fatal_transport_error_fails_at_once_without_retrying() {
        let env = FakeEnv::ao3(vec![Err(GetError::Fatal("builder error".to_string())), ok("never reached", "u")]);

        let err = block_on(get_with_retry(&env, "https://x/")).unwrap_err();

        assert_eq!(err, "builder error");
        assert_eq!(env.gets.lock().unwrap().len(), 1);
    }

    #[test]
    fn gives_up_after_the_last_attempt_naming_the_last_error() {
        let env = FakeEnv::ao3(vec![status(500), status(502), retryable("connection refused"), status(503), ok("late", "u")]);

        let err = block_on(get_with_retry(&env, "https://x/")).unwrap_err();

        assert_eq!(err, format!("AO3 fetch failed after {AO3_FETCH_MAX_ATTEMPTS} attempts: HTTP 503 Service Unavailable"));
        assert_eq!(env.gets.lock().unwrap().len() as u32, AO3_FETCH_MAX_ATTEMPTS);
    }

    #[test]
    fn backoff_doubles_between_attempts() {
        let mut env = FakeEnv::ao3(vec![status(500), status(500), status(500), ok("body", "u")]);
        env.ao3_base_backoff = Duration::from_millis(20);
        let started = std::time::Instant::now();

        block_on(get_with_retry(&env, "https://x/")).unwrap();

        // 20 + 40 + 80 ms of sleeping before the fourth attempt succeeds.
        assert!(started.elapsed() >= Duration::from_millis(140), "{:?}", started.elapsed());
    }

    // ---- reqwest_get against a loopback server ----

    /// Serves each canned response to one connection in turn and returns the
    /// raw request text it saw for each.
    fn serve(responses: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = vec![];
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                seen.push(String::from_utf8_lossy(&buf[..n]).to_string());
                stream.write_all(response.as_bytes()).unwrap();
            }
            seen
        });
        (base, handle)
    }

    fn http(status_line: &str, extra_headers: &str, body: &str) -> String {
        format!("HTTP/1.1 {status_line}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    #[test]
    fn reqwest_get_sends_the_browser_user_agent_and_follows_a_redirect() {
        let (base, server) = serve(vec![
            http("302 Found", "Location: /works/1/chapters/2\r\n", ""),
            http("200 OK", "", "<html>chapter</html>"),
        ]);

        let resp = block_on_io(reqwest_get(&format!("{base}/works/1"))).unwrap();

        assert!(resp.status.is_success());
        assert_eq!(resp.body, "<html>chapter</html>");
        assert_eq!(resp.final_url, format!("{base}/works/1/chapters/2"));
        let requests = server.join().unwrap();
        for request in requests {
            let lower = request.to_lowercase();
            assert!(lower.contains(&format!("user-agent: {}", AO3_USER_AGENT.to_lowercase())), "{request}");
        }
    }

    #[test]
    fn reqwest_get_reports_a_server_error_status_without_reading_it_as_a_failure() {
        let (base, server) = serve(vec![http("503 Service Unavailable", "", "Shields are up!")]);

        let resp = block_on_io(reqwest_get(&format!("{base}/works/1"))).unwrap();
        server.join().unwrap();

        assert_eq!(resp.status.as_u16(), 503);
        assert!(resp.body.is_empty());
    }

    #[test]
    fn reqwest_get_classifies_a_refused_connection_as_retryable() {
        // Bind then drop, so the port is closed.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

        let err = block_on_io(reqwest_get(&format!("http://127.0.0.1:{port}/"))).unwrap_err();

        assert!(matches!(err, GetError::Retryable(_)), "{err:?}");
    }
}
