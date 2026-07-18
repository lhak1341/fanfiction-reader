---
id: 005
title: UI/UX layout for search/filter view and reader view
label: wayfinder:prototype
status: closed
assignee: claude-session
blocked_by: []
parent: ../map.md
---

## Question

What should the two core screens look like and behave like: the search/filter view (fandom/category browse mirroring FFN, with word-count/status/favorites-sort filters and mark-as-read hiding layered on top) and the reader view (chapter content as continuous scroll, chapter-by-chapter nav, manual mark-as-read toggle)? Use /prototype to build a cheap, rough, concrete artifact (wireframe or stub UI) for the user to react to, then lock the layout into the spec.

## Resolution

Built an HTML wireframe (3 structurally different variants each for Search and Reader), reviewed live with the user, iterated through several rounds of feedback. Locked layout below. Asset: HTML/CSS/JS prototype at `/private/tmp/claude-501/-Users-lhak-workspace-github-com-lhak1341-fanfiction/f989507f-e69c-4eff-a090-1d951d089fb3/scratchpad/ffn-reader-prototype.html` (throwaway — not meant to ship, fold the decisions below into the real Svelte build).

**Top-level nav**: two persistent tabs, **Search** and **Bookmarks**. Reader is not a tab — it opens full-screen when a story is clicked from either list, with a Back control returning to whichever tab you came from.

**Search screen**:
- Filter bar: a dedicated **Fandom picker** (separate control, not the text search) — click to open a popover with a type-to-filter list of fandoms. Fandoms can be **pinned** (star toggle inline in the list) so frequently-read fandoms float to a "Pinned" section at the top of the popover, ahead of the alphabetical/full list.
- Free-text search box is separate from the fandom picker — filters by **title, author, or summary** only, not fandom.
- Word-count minimum (Any / 40K+ / 100K+), status (Any / Complete / In progress), sort (Favorites / Updated / Words), and a "hide read & dismissed" checkbox — all as originally scoped.
- **List/card view toggle** — both presentations are real, user-facing features, not competing mockups:
  - **List view**: one row per story — title + author/status stacked on the left, a **single-line truncated description** in the middle (row height does not grow to fit it), word count + favorites on the right, then row actions (mark-read, dismiss, bookmark icons).
  - **Card view**: fandom tag, title, full summary, rating/words/favorites tags, status pill, and the same three row actions in the card footer.

**Bookmarks screen** (new, surfaced during this ticket — schema amended, see [Data model ticket](003-data-model-sqlite-schema.md) Amendment 2): same list/card toggle and rendering as Search, but:
- Its own **Fandom picker**, defaulting to "All fandoms" — scoped to only the fandoms actually present among bookmarked stories, not the global fandom list.
- List rows show fandom (since entries span fandoms here, unlike Search where the fandom is already the active filter).
- **No dismiss action** on rows/cards — dismissing a story you've bookmarked isn't a meaningful action; only mark-read and un-bookmark are offered.

**Reader screen**: immersive layout — progress bar across the top, chapter prose in a centered serif column, no persistent chrome by default. Corner controls fade in on hover only (stay out of the way while reading):
- Top-left: **Back** (to Search or Bookmarks, whichever you came from) and a **Chapters** button that opens a slide-in drawer listing all chapters (click to jump), closable via an X, overlay click, or Escape.
- Top-right: **Bookmark** and **Dismiss** toggles.
- Bottom-right: floating **mark-as-read** button.
- Left/right edges of the reading area are click zones for previous/next chapter (per the chapter-by-chapter nav decision).
