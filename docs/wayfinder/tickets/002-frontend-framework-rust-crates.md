---
id: 002
title: Frontend framework and core Rust crate choices for the Tauri app
label: wayfinder:grilling
status: closed
assignee: claude-session
blocked_by: []
parent: ../map.md
---

## Question

Which frontend framework (React, Svelte, Solid, vanilla+Bun, etc.) will the Tauri webview UI use, and which core Rust crates does the spec mandate — HTML parsing (e.g. `scraper`/`html5ever`), local storage (e.g. `rusqlite`), and any others needed for the cache/reader? Decide with the user via /grilling, weighing what's simplest to build and maintain for a solo macOS app.

Note: per [Cloudflare/session-handling strategy](001-cloudflare-session-handling.md), fetching goes through Tauri's WebView + injected JS, not a Rust HTTP client hitting FFN directly — drop `reqwest`-for-Cloudflare-bypass from scope here; a lightweight HTTP client may still be worth picking for warm-session re-fetches, but it's secondary, not the primary fetch path.

## Resolution

**Frontend**: Svelte + Bun (Tauri's official Svelte template) — smaller bundle, less boilerplate than React for two simple screens (search/filter list, reader scroll).

**Rust crates**:
- `tauri` — app shell
- `scraper` — HTML parsing, paired with the FanFicFare-ported field-extraction logic ([ticket #4](004-fanficfare-port-scope.md))
- `rusqlite` — local SQLite storage (sync, simplest fit for Tauri commands vs async `sqlx`)
- `serde` / `serde_json` — Rust-backend ↔ Svelte-frontend data passing via Tauri commands
- `reqwest` (secondary) — warm-session re-fetches only, not primary fetch path (webview handles that per ticket #1)
