# Longform

A local-first reader for fanfiction imported from multiple sources (FanFiction.net, Archive of Our Own).

## Language

**Source**:
One of the two origin sites a Story can be imported from — FanFiction.net (`ffn`) or Archive of Our Own (`ao3`). Each has its own markup, tagging vocabulary, and fetch mechanics.

**Fandom**:
The canonical grouping a Story belongs to (e.g. "Naruto", "Harry Potter"), stored as `stories.fandom` — one label per Story, not per Source. The same fandom can be searched and imported from either Source; sources don't always agree on the fandom's exact label, which is why import force-applies one chosen label per search rather than trusting each Source's own text.

There is deliberately no cross-Source fandom-identity table (e.g. mapping a canonical fandom to each Source's own tag/label). What exists instead, by design, not as gaps to close: `rename_fandom` (`queries.rs`) is a manual one-time repair for buckets that ended up wrong before the force-apply above existed, or for AO3 crossover works whose `fandom` is a comma-joined multi-fandom string; `fandom.ts`'s AO3 tag map is a client-side, per-user preference (typed once, reused after) because AO3 has no queryable fandom→tag index (`robots.txt` blocks `/autocomplete/`) and this isn't fetched data; `lastSearchSource` (`+page.svelte`) is a session-scoped guess for which Source "Load more" should continue when the fandom alone doesn't say. Revisit only if a concrete new pain point (not just tidiness) shows up.

**FicSource**:
The seam unifying the two Sources' parsing and fetch-dispatch operations — resolving a fetched page to either a chapter or a listing, and parsing/persisting either shape. Two parallel implementations: `Ffn`/`Ao3` (`src-tauri/src/source.rs`) on the backend, `ffnAdapter`/`ao3Adapter` (`src/lib/ficSource.ts`) on the frontend for chapter-URL-building and search-continuation. Deliberately narrow: it does *not* cover DB upsert (`upsert_story`/`upsert_ao3_story` stay separate — the two sources' metadata shapes don't overlap enough to share a schema-facing signature), FFN's category-index fetch (no AO3 equivalent), or `SearchModal.svelte`'s input UI (a genuine UX difference, not duplication).
_Avoid_: adapter (for the whole concept — "adapter" refers to one of the two `Ffn`/`Ao3` (or `ffnAdapter`/`ao3Adapter`) impls _behind_ FicSource, not the seam itself), source handler.
