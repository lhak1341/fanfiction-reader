---
id: 001
title: Cloudflare/session-handling strategy for m.fanfiction.net
label: wayfinder:research
status: closed
assignee: claude-session
blocked_by: []
parent: ../map.md
---

## Question

How will the app reliably fetch pages from m.fanfiction.net given Cloudflare's bot protection? Specifically: does personal-scale traffic (one user, occasional fetches) actually trigger Cloudflare challenges in practice, does the mobile subdomain have lighter protection than the desktop site, is a cookie-jar/session bootstrap needed before plain `reqwest` calls work, and could Tauri's own webview be used as the fetch mechanism (inheriting a real browser fingerprint) instead of a bare HTTP client for pages that get challenged?

Resolve by researching FanFicFare's known Cloudflare issues (e.g. github.com/JimmXinu/FanFicFare/issues/703) and any documented workarounds, and note what strategy the spec should mandate.

## Resolution

Bare HTTP clients (`reqwest`, `curl`, `cloudscraper`-style solvers) are unreliable against FFN's Cloudflare "Version 2" JS challenge — even with a copied `cf_clearance` cookie and matching User-Agent, TLS/connection fingerprinting still blocks non-browser traffic (source: github.com/seanmonstar/reqwest/discussions/2227, github.com/JimmXinu/FanFicFare/issues/622, /703, /614). The mobile subdomain (`m.fanfiction.net`) does **not** get lighter protection — same challenge loop as desktop, contrary to this ticket's original hunch.

**Strategy the spec mandates**: fetch through Tauri's own WebView (WKWebView on macOS), not a bare HTTP client. The app already renders its UI in a webview — reuse it (or a background instance) to navigate pages and pull HTML via injected JS. Being a real browser engine, it solves the JS challenge naturally and persists Cloudflare session cookies in its own cookie store across launches, with no manual cookie-copying or fingerprint mismatch. A bare HTTP client (`reqwest`) remains usable only as a lightweight fallback for re-fetches inside an already-warm session — not as the primary fetch path.

Impact on other tickets: [Frontend framework and core Rust crate choices](002-frontend-framework-rust-crates.md) needs its `reqwest`/Cloudflare-bypass assumption revised — HTML parsing crate choice still stands, but the fetch mechanism is webview-navigation + JS injection, not a Rust HTTP client hitting FFN directly.

## Empirical validation (post-map, during scaffolding)

Confirmed against the live site, not just research: a Tauri `WebviewWindowBuilder` pointed at `https://www.fanfiction.net/s/8067258/1/` loaded the real story (`Worth Dying For Chapter 1: Prologue: Memento Mori`, 47,060 bytes of HTML, `#storytext`/`content_wrapper_inner` both present) with no Cloudflare challenge. Two ACL gotchas hit along the way, now fixed in the scaffold and worth remembering for later Tauri commands:

1. `window.__TAURI__` isn't injected by default in Tauri v2 — needed `"withGlobalTauri": true` under `app` in `tauri.conf.json` for the injected JS to call back into Rust at all.
2. Once the app defines any custom permission (`src-tauri/permissions/*.toml`), Tauri's ACL enforces strict allow-listing for **all** app-defined commands, not just remote-origin ones — `fetch_page` (invoked from the local main window) needed its own permission entry too, not just `report_fetch_result` (invoked from the remote fanfiction.net window). See `src-tauri/permissions/main-commands.toml` and `report-fetch-result.toml`.
3. A third, subtler bug surfaced while testing chapter navigation: the blocked-page detection originally scanned the *entire* page (HTML + title) for phrases like "just a moment" — and a story's own prose can innocently contain that exact phrase, producing a false "Cloudflare blocked this" error on a fetch that actually succeeded. Fixed by scoping the phrase checks to the `<title>` only (a real FFN chapter title can't plausibly collide with a Cloudflare block-page title), keeping only the `cf-browser-verification` CSS-class check against the full page (a structural marker, not free-form text).
