// FicSource seam (architecture review candidate 1, TS-side pass, 2026-07-19):
// continuing a fandom search ("Load more") is the one spot where FFN/AO3
// dispatch still leaks across call sites on the frontend, collapsed into one
// adapter lookup per source. Building a chapter's fetch URL used to live here
// too; it moved to Rust's `FicSource::chapter_url` (architecture review,
// 2026-10-07) so the frontend sends only `(storyId, chapterNum)`. Everything
// else (searchFfnAndWait/searchAo3AndWait, fetchChapterAndWait, SearchModal's
// input UI) stays exactly as it is: those are either already source-generic
// or a genuine UI difference, not duplication. See CONTEXT.md's `FicSource`
// entry for the matching Rust-side seam this mirrors.

import * as api from "./api";
import { getAo3Tag } from "./fandom";
import type { BrowseFetched, StorySource, StoryStatus } from "./types";

export interface FicSourceAdapter {
  /// Continues paging the current fandom search ("Load more").
  loadMore(
    fandom: string,
    minWordCount: number,
    status: StoryStatus | "any"
  ): Promise<BrowseFetched>;
}

const ffnAdapter: FicSourceAdapter = {
  loadMore(fandom, minWordCount, status) {
    return api.searchFfnAndWait(fandom, minWordCount, status);
  },
};

const ao3Adapter: FicSourceAdapter = {
  loadMore(fandom, minWordCount, status) {
    // No cached tag index exists (robots.txt blocks AO3's /autocomplete/,
    // §8) — the tag has to have been saved by a prior modal search.
    const tag = getAo3Tag(fandom);
    if (!tag) {
      return Promise.reject(
        new Error(`No AO3 tag saved for "${fandom}" yet — search it once via the modal first.`)
      );
    }
    return api.searchAo3AndWait(tag, fandom, minWordCount, status);
  },
};

export function ficSourceAdapter(source: StorySource): FicSourceAdapter {
  return source === "ao3" ? ao3Adapter : ffnAdapter;
}
