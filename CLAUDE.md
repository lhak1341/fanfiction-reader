# Longform (FanFiction Reader)

- DB lives at `~/Library/Application Support/com.lhak1341.longform/longform.db` — safe to inspect/edit directly with `sqlite3` while `bun run tauri dev` is running.
- `docs/SPEC.md` is the living spec — read the relevant section before touching scraping/fetch/data-model code, and update it when behavior changes.
- No `docs/adr/` — architectural decisions live in `docs/wayfinder/map.md` + tickets (macro) and inline `// architecture review candidate N` comments (micro) — grep that phrase before proposing a refactor.
- `temp/` (repo root, gitignored) is this project's scratch/output dir, not OS tmp.
- Background `bun run tauri dev`/vite processes are invisible to `ps aux` — use `lsof -i :1420` / `lsof +D src-tauri/target` instead. This shell has no `timeout` — use `run_in_background` + manual sleep/check.
- This app can't be screenshot-verified (Tauri's WKWebView isn't CDP-attachable, no `window.__TAURI__` outside the real shell) — for UI changes, ask the user to check the running app directly.
- Backend (Rust/Tauri) conventions: see `src-tauri/CLAUDE.md`. Frontend (Svelte) conventions: see `src/CLAUDE.md`. Test-writing conventions: see `docs/TESTING.md`. Live-fetch verification: see `docs/FETCH_TESTING.md`.
