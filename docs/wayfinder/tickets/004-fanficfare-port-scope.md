---
id: 004
title: FanFicFare parsing-logic port scope
label: wayfinder:research
status: closed
assignee: claude-session
blocked_by: []
parent: ../map.md
---

## Question

Which exact part(s) of FanFicFare's source handle fanfiction.net/m.fanfiction.net (its adapter module and any shared base-adapter logic) need to be studied and ported to Rust? Identify what fields/behavior it extracts (story metadata, chapter list, chapter content, pagination handling) so the spec can scope the Rust port precisely rather than porting the whole library. Produces a linked research summary as the asset.

## Resolution

Source: `fanficfare/adapters/adapter_fanfictionnet.py` in github.com/JimmXinu/FanFicFare (class `FanFictionNetSiteAdapter`, `BaseSiteAdapter` subclass). This one file is the whole port target — no other adapter or shared base-adapter logic is fanfiction.net-specific.

**Key finding**: the adapter rewrites `m.fanfiction.net` URLs to `www.fanfiction.net` in its constructor before parsing anything (`if "https://m." in self.origurl: self.origurl = self.origurl.replace("https://m.","https://www.")`). It never parses mobile markup — confirms [ticket #1](001-cloudflare-session-handling.md)'s finding that `m.` gets no special treatment, and means **our port scope targets `www.fanfiction.net` markup**, not `m.`, even though the user reads via `m.` in-browser. One page shape to port, matches what FanFicFare already proved out.

**What to port**, mapped to our [`stories`/`chapters` schema](003-data-model-sqlite-schema.md):

1. **Story metadata parser** — the story's chapter-1 page has a single delimited text line inside `div#content_wrapper_inner`, e.g.: `Rated: Fiction T - English - Adventure/Sci-Fi - Naruto U. - Chapters: 22 - Words: 114,414 - Reviews: 395 - Favs: 779 - Follows: 835 - Updated: 03-21-13 - Published: 04-28-12 - id: 8067258`. Parsed by splitting on `' - '` and mapping known keys (`metakeys` dict: `Words`→`numWords`, `Favs`→`favs`, `Follows`, `Reviews`; `Rated`/language/genre/characters parsed positionally before the keyed part; `Chapters`, `Updated`, `Published`, `id` handled separately). Maps directly to `stories.rating`, `.language`, `.genre`, `.word_count`, `.favorite_count`, `.chapter_count`.
2. **Status detection** — "Complete" is detected separately from the metadata line (own regex/text check), not one of the keyed fields. Maps to `stories.status`.
3. **Chapter list** — `<select name="chapter">` dropdown; each `<option>` gives a chapter number + title + URL. Maps to populating `chapters` rows (`chapter_num`, `title`).
4. **Chapter content** — `getChapterText(url)` fetches each chapter page and extracts the story body div (FFN's standard `#storytext` container). Maps to `chapters.content`.
5. **Static reference data** — `ffnetgenres` (fixed list of valid genre strings) and `ffnetpluscategories` (edge-case category names containing `+`) are small static lists worth porting verbatim for validation/display, not scraped.

**Explicitly out of port scope**: FanFicFare's other-site adapters, its epub/mobi/output-format code, its config/`defaults.ini` system, its calibre-plugin/GUI, cover-image handling, author-page/series-listing scraping — none of that was requested and none of it is needed for this app's search→filter→read flow.

## Empirical validation (post-map, during scaffolding)

Ported and tested against real markup captured by the [Cloudflare fetch spike](001-cloudflare-session-handling.md) (`app/src-tauri/src/ffn.rs`, not guesses from the Python source). Real selectors differ from what the ticket's research summary implied:

- Title/author/summary/genre/rating/language/word-count/favorites all live inside `#profile_top`, not `#content_wrapper_inner` (that id wraps a much larger region — profile_top is the specific block within it).
- Title: `b.xcontrast_txt`. Author: `a.xcontrast_txt[href^="/u/"]`. Summary: the one direct-child `div.xcontrast_txt` of `#profile_top`. Metadata line: `span.xgray`.
- **Fandom isn't in the metadata line at all** — missed in the original research. It's the last link in the `#pre_story_links` breadcrumb (e.g. "Anime/Manga › Naruto"). Added as its own selector.
- Chapter list and content selectors (`select[name="chapter"] option`, `#storytext`) matched the research as expected.

Verified end-to-end against `/s/8067258/1/` ("Worth Dying For"): every field matched the live page exactly — title, author, fandom, rating (T), genre (Adventure/Sci-Fi), language (English), word count (76,552), favorites (1,758), all 8 chapter titles in order, and chapter 1's `#storytext` content (9,247 bytes, clean paragraph HTML).
