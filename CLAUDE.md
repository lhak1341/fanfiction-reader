# Longform (FanFiction Reader)

- DB lives at `~/Library/Application Support/com.lhak1341.longform/longform.db` — safe to inspect/edit directly with `sqlite3` while `bun run tauri dev` is running.
- `docs/SPEC.md` is the living spec — read the relevant section before touching scraping/fetch/data-model code, and update it when behavior changes.
- Live-fetch verification (confirming scraping behavior against FFN/AO3) must use `python3 urllib`, not `curl` — blocked by sandbox policy. AO3 origin is flaky (timeouts, `HTTP 525`) even when not bot-blocked — retry 3-4x before concluding a fetch failed.
- `cargo check`/`cargo test` through the `rtk` hook can print a misleadingly terse summary (e.g. "cargo build (0 crates compiled)") that hides real errors/warnings — use `rtk proxy cargo <args>` when verifying Rust changes in `src-tauri`.
- Before trusting a schema migration in `db.rs`, copy the real DB to scratch and check `PRAGMA integrity_check`/`PRAGMA foreign_key_check`/row counts against the copy — don't just trust the SQL in isolation.
