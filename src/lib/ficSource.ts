// FicSource seam (architecture review candidate 1, TS-side pass, 2026-07-19):
// the two spots where FFN/AO3 dispatch actually leaked across call sites —
// building a chapter's fetch URL, and continuing a fandom search ("Load
// more") — collapsed into one adapter lookup per source. Everything else
// (searchFfnAndWait/searchAo3AndWait, fetchChapterAndWait, SearchModal's
// input UI) stays exactly as it is: those are either already source-generic
// or a genuine UI difference, not duplication. See CONTEXT.md's `FicSource`
// entry for the matching Rust-side seam this mirrors.

import * as api from "./api";
import { getAo3Tag } from "./fandom";
import type { BrowseFetchedEvent, StoryRow, StorySource, StoryStatus } from "./types";

export interface FicSourceAdapter {
  /// Builds the fetch URL for a given chapter. AO3 chapters beyond the
  /// first need their own opaque `sourceChapterId`
  /// (`/works/{id}/chapters/{id}`, SPEC.md §8) — a one-shot with no chapter
  /// id known yet falls back to the bare `/works/{id}` page.
  chapterUrl(story: StoryRow, chapterNum: number, sourceChapterId: number | null | undefined): string;

  /// Continues paging the current fandom search ("Load more").
  loadMore(fandom: string, minWordCount: number, status: StoryStatus | "any"): Promise<BrowseFetchedEvent>;
}

const ffnAdapter: FicSourceAdapter = {
  chapterUrl(story, chapterNum) {
    return `https://www.fanfiction.net/s/${story.sourceStoryId}/${chapterNum}/`;
  },
  loadMore(fandom, minWordCount, status) {
    return api.searchFfnAndWait(fandom, minWordCount, status);
  }
};

const ao3Adapter: FicSourceAdapter = {
  chapterUrl(story, _chapterNum, sourceChapterId) {
    return sourceChapterId
      ? `https://archiveofourown.org/works/${story.sourceStoryId}/chapters/${sourceChapterId}`
      : `https://archiveofourown.org/works/${story.sourceStoryId}`;
  },
  loadMore(fandom, minWordCount, status) {
    // No cached tag index exists (robots.txt blocks AO3's /autocomplete/,
    // §8) — the tag has to have been saved by a prior modal search.
    const tag = getAo3Tag(fandom);
    if (!tag) {
      return Promise.reject(new Error(`No AO3 tag saved for "${fandom}" yet — search it once via the modal first.`));
    }
    return api.searchAo3AndWait(tag, fandom, minWordCount, status);
  }
};

export function ficSourceAdapter(source: StorySource): FicSourceAdapter {
  return source === "ao3" ? ao3Adapter : ffnAdapter;
}
