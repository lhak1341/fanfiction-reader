# Live-fetch verification

- Confirming scraping behavior against FFN/AO3 must use `python3 urllib`, not `curl` — blocked by sandbox policy.
- AO3 origin is flaky (timeouts, `HTTP 525`) even when not bot-blocked — retry 3-4x before concluding a fetch failed.
- FFN itself is fully Cloudflare-blocked (403) for *any* bare client, `urllib` included, even with a real desktop UA header — there's no way around this without the app's own webview.
- To verify or backfill a specific parsed field against real data, run the actual Rust parser against saved live HTML via a throwaway `#[test]` (`--nocapture`) rather than hand-deriving the value — it's the ground truth and won't have a transcription bug.
- To capture real FFN markup for a fixture/test: trigger the fetch in the running app (`bun run tauri dev`), then copy `~/Library/Application Support/com.lhak1341.longform/debug-last-fetch.html` (or `debug-last-ao3-fetch.html` for AO3) *before* the next fetch overwrites it — this file always holds the most recently fetched page's raw HTML.
- Diagnose a failed FFN fetch from `debug-last-fetch.html`, not the error string: it holds the last page the webview *reported*, written before the Cloudflare check. A ~2 KB page reading "Story does not have any chapters" is FFN's own refusal (served for a chapter that loaded fine in a browser; the terminal shows `could not find #profile_top`); a stale mtime means no report arrived at all (timeout or interrupted).
