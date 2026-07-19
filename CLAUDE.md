# Longform (FanFiction Reader)

- DB lives at `~/Library/Application Support/com.lhak1341.longform/longform.db` — safe to inspect/edit directly with `sqlite3` while `bun run tauri dev` is running.
- `docs/SPEC.md` is the living spec — read the relevant section before touching scraping/fetch/data-model code, and update it when behavior changes.
- Live-fetch verification (confirming scraping behavior against FFN/AO3) must use `python3 urllib`, not `curl` — blocked by sandbox policy. AO3 origin is flaky (timeouts, `HTTP 525`) even when not bot-blocked — retry 3-4x before concluding a fetch failed.
- `cargo check`/`cargo test` through the `rtk` hook can print a misleadingly terse summary (e.g. "cargo build (0 crates compiled)") that hides real errors/warnings — use `rtk proxy cargo <args>` when verifying Rust changes in `src-tauri`. The same hook can also mangle plain `ls`/`grep` output on files under `target/` into truncated garbage — fall back to a Python file read if output looks wrong.
- Before trusting a schema migration in `db.rs`, copy the real DB to scratch and check `PRAGMA integrity_check`/`PRAGMA foreign_key_check`/row counts against the copy — don't just trust the SQL in isolation.
- Adding a new Tauri command needs two registrations: `invoke_handler!` in `lib.rs` AND `commands.allow` in `permissions/main-commands.toml` — missing the second compiles fine but fails at runtime with "not allowed", no compile warning.
- After editing `permissions/*.toml`/`capabilities/*.json`, don't trust a fast "Finished" build — verify the change actually landed (e.g. grep `target/debug/build/app-*/out/capabilities.json`) before assuming it's applied.
- Background `bun run tauri dev`/vite processes are invisible to `ps aux` from the Bash tool but hold real ports/locks — use `lsof -i :1420` / `lsof +D src-tauri/target` to find and kill them, not `ps`.
- `timeout` isn't available in this shell (macOS, no coreutils) — use `run_in_background` + manual sleep/check instead.
