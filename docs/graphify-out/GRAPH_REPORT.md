# Graph Report - docs  (2026-10-07)

## Corpus Check
- 9 files · ~12,737 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 52 nodes · 54 edges · 8 communities (6 shown, 2 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `54a580c4`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- map.md
- 003-data-model-sqlite-schema.md
- Longform (FanFiction Reader) — App Spec
- 6. UI/UX
- FFN Reader App Spec
- 4. Scraping
- FETCH_TESTING.md
- TESTING.md

## God Nodes (most connected - your core abstractions)
1. `Longform (FanFiction Reader) — App Spec` - 9 edges
2. `6. UI/UX` - 6 edges
3. `FFN Reader App Spec` - 6 edges
4. `4. Scraping` - 5 edges
5. `8. Multi-source planning: Archive of Our Own (in progress)` - 2 edges
6. `Live-fetch verification` - 1 edges
7. `1. Scope` - 1 edges
8. `2. Architecture` - 1 edges
9. `3. Fetching & Cloudflare strategy` - 1 edges
10. `Story page (`parse_story_page`)` - 1 edges

## Surprising Connections (you probably didn't know these)
- None detected - all connections are within the same source files.

## Communities (8 total, 2 thin omitted)

### Community 0 - "map.md"
Cohesion: 0.19
Nodes (8): Empirical validation (post-map, during scaffolding), Question, Resolution, Question, Resolution, Empirical validation (post-map, during scaffolding), Question, Resolution

### Community 1 - "003-data-model-sqlite-schema.md"
Cohesion: 0.20
Nodes (8): Amendment, Amendment 2, Amendment 3, Empirical validation (post-map, during scaffolding), Question, Resolution, Question, Resolution

### Community 2 - "Longform (FanFiction Reader) — App Spec"
Cohesion: 0.25
Nodes (8): 1. Scope, 2. Architecture, 3. Fetching & Cloudflare strategy, 5. Data model (SQLite), 7. Decision trail, 8. Multi-source planning: Archive of Our Own (in progress), Longform (FanFiction Reader) — App Spec, Progress & next steps (as of 2026-07-19)

### Community 3 - "6. UI/UX"
Cohesion: 0.33
Nodes (6): 6. UI/UX, Bookmarks, Reader, Reader typography panel, Search, Search modal (`SearchModal.svelte`, generalized 2026-07-19 from `SearchFfnModal.svelte`)

### Community 4 - "FFN Reader App Spec"
Cohesion: 0.33
Nodes (6): Decisions so far, Destination, FFN Reader App Spec, Not yet specified, Notes, Out of scope

### Community 5 - "4. Scraping"
Cohesion: 0.40
Nodes (5): 4. Scraping, Browse/category listing page (`parse_browse_page`), Category fandom index (`parse_fandom_index`), FFN discovery URL construction (`build_browse_url`), Story page (`parse_story_page`)

## Knowledge Gaps
- **38 isolated node(s):** `Live-fetch verification`, `1. Scope`, `2. Architecture`, `3. Fetching & Cloudflare strategy`, `Story page (`parse_story_page`)` (+33 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **2 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Longform (FanFiction Reader) — App Spec` connect `Longform (FanFiction Reader) — App Spec` to `map.md`, `6. UI/UX`, `4. Scraping`?**
  _High betweenness centrality (0.509) - this node is a cross-community bridge._
- **Why does `6. UI/UX` connect `6. UI/UX` to `Longform (FanFiction Reader) — App Spec`?**
  _High betweenness centrality (0.173) - this node is a cross-community bridge._
- **Why does `FFN Reader App Spec` connect `FFN Reader App Spec` to `map.md`?**
  _High betweenness centrality (0.173) - this node is a cross-community bridge._
- **What connects `Live-fetch verification`, `1. Scope`, `2. Architecture` to the rest of the system?**
  _38 weakly-connected nodes found - possible documentation gaps or missing edges._