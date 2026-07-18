---
label: wayfinder:map
status: closed
created: 2026-07-18
closed: 2026-07-18
spec: ../SPEC.md
---

# FFN Reader App Spec

## Destination

A spec to hand off for a solo, local-only Tauri (Rust + Bun) desktop app, macOS-first, that reads fanfiction from fanfiction.net (the user's browsing entry point is `m.fanfiction.net`, but the app scrapes `www.fanfiction.net` markup internally — see the Cloudflare and FanFicFare-port decisions below). The app scrapes search/browse results and story chapters (via a Rust port of FanFicFare's parsing logic), renders chapters in-app as a continuous scroll, caches fetched content locally for offline reading, and layers filters FFN's own site lacks — manual mark-as-read, dismiss ("not for me"), bookmark, minimum word count, sort by favorites, status (complete/in-progress), pinned fandoms — on top of FFN's search/browse. Three screens: Search, Bookmarks, and an immersive Reader.

## Notes

- Domain: personal fanfiction reading/scraping app.
- Skills to consult: /grilling, /domain-modeling, /prototype as tickets call for them.
- Standing preferences locked during charting:
  - Solo, local-only — not distributed to other users. Caps legal exposure from ToS/Cloudflare scraping and simplifies scope (no auth/multi-user).
  - Scraper: port FanFicFare's parsing logic to Rust rather than shelling out to the Python tool — keeps the app pure Rust/Tauri.
  - Reading is in-app (not a link-out launcher): each chapter renders as one continuous scroll, navigation is chapter-by-chapter, no artificial in-chapter pagination.
  - Mark-as-read is a manual toggle (not automatic) — ground truth stays accurate across abandoned/reread fics.
  - Dismiss ("not for me") is a manual toggle separate from mark-as-read — for stories rejected without finishing (e.g. bailed after chapter 1); hides from default results same as read stories do.
  - Bookmark is a third independent manual flag (not read, not dismissed) — browsable via its own Bookmarks screen/tab, fandom-agnostic by default.
  - Caching fetched chapters locally doubles as offline reading; re-fetch only on explicit "check for updates."

## Decisions so far

- [Cloudflare/session-handling strategy for m.fanfiction.net](tickets/001-cloudflare-session-handling.md) — fetch via Tauri's WebView (navigation + JS injection), not a bare HTTP client; bare `reqwest` fails Cloudflare's TLS fingerprinting even with a copied cookie, and the mobile subdomain gets no lighter protection than desktop.
- [Frontend framework and core Rust crate choices for the Tauri app](tickets/002-frontend-framework-rust-crates.md) — Svelte + Bun frontend; Rust crates: `tauri`, `scraper`, `rusqlite`, `serde`/`serde_json`, `reqwest` (secondary, warm-session only).
- [Data model / SQLite schema for stories, chapters, read-state, filter presets](tickets/003-data-model-sqlite-schema.md) — 4-table schema (`stories`, `chapters`, `read_state`, `filter_presets`); `read_state` amended three times: `dismissed` (not-for-me), `bookmarked`, and `last_opened_at` (feeds a Bookmarks "Continue Reading" surface) — all alongside `is_read` as independent per-story flags/timestamps. Also added resume-reading position and saved filter presets.
- [FanFicFare parsing-logic port scope](tickets/004-fanficfare-port-scope.md) — port target is one file, `adapter_fanfictionnet.py`; scrapes `www.fanfiction.net` markup (FanFicFare itself rewrites `m.` URLs to `www.` before parsing — confirms mobile gets no special treatment). Scope: metadata-line parser, status detection, chapter-list `<select>` parsing, `#storytext` content extraction.
- [UI/UX layout for search/filter view and reader view](tickets/005-ui-ux-layout-prototype.md) — prototyped and locked over several review rounds: Search + Bookmarks tabs (each with list/card toggle), a pinnable fandom picker separate from free-text search, and an immersive Reader with hover-only corner chrome (back, chapter drawer, bookmark, dismiss) plus edge-click chapter nav. Surfaced the bookmark feature, which fed back into the schema.

## Not yet specified

_(none — graduated into [SPEC.md](../SPEC.md) §4's maintenance note when the spec was assembled)_

## Out of scope

- Distribution/sharing the app to other users — ruled out by the solo/local-only decision made while charting.
- Platforms beyond macOS (Windows/Linux/mobile) — v1 is macOS-first only.
- In-app writing, reviewing, or uploading to FFN — reading only.
- Automatic mark-as-read — manual toggle chosen for v1; auto-suggest/auto-mark deferred as a possible v2 idea, not part of this spec.
