// Reader screen state + logic, split out of Reader.svelte (architecture
// review candidate 1, 2026-07-23) — same rationale as searchScreen.svelte.ts/
// bookmarksScreen.svelte.ts: a plain .svelte.ts class instead of a .svelte
// component, so the cancellation-safe prefetch chain and scroll-position
// bookkeeping are reachable from Vitest instead of only through a mounted
// DOM. Deliberately doesn't own the scroll DOM node itself — `loadChapter`
// takes/returns plain numbers, and Reader.svelte does the actual
// tick()+rAF+scrollTop DOM work. Typography prefs (font panel) and the
// nav-carousel derived math stay in Reader.svelte too: both are thin
// presentational concerns, not the tricky part this split is for.
//
// Architecture review candidate 5 (2026-10-07), rejected: extracting the nav
// math / prose-style derivation into a pure module would be a shallow module
// with one caller, and a jsdom mount test can't reproduce the one real edge
// case (the rAF scroll-restore clamping — jsdom does no layout). Reopen if a
// scroll or nav bug actually lands in Reader.svelte.

import * as api from "./api";
import { ensureChapterCached } from "./chapterCache";
import { jitterDelay, sleep } from "./pacedLoop";
import { toggleStoryFlag } from "./storyFlags";
import type { ChapterContent, ChapterListItem, StoryRow } from "./types";

export class ReaderScreen {
  story = $state<StoryRow | undefined>(undefined);
  chapters = $state<ChapterListItem[]>([]);
  currentChapterNum = $state(0);
  content = $state<ChapterContent | undefined>(undefined);
  loading = $state(true);
  fetchError = $state("");
  toggleError = $state("");

  // Remembers scroll position per chapter for this reading session (not
  // persisted) — so paging back to a chapter you were partway through
  // resumes where you left off, instead of always landing at the top.
  scrollPositions = new Map<number, number>();

  // Bumped on every new loadChapter; a superseded load and a running prefetch
  // chain both check it and bail the moment a newer navigation takes over.
  private loadGen = 0;
  private inFlight?: { num: number; promise: Promise<void> };

  private readonly storyId: number;
  private readonly onChanged?: () => void;

  constructor(storyId: number, onChanged?: () => void) {
    this.storyId = storyId;
    this.onChanged = onChanged;
  }

  // Called when Reader unmounts. Bumps `loadGen`, so the prefetch chain and
  // any load still in flight bail at their next check: without it the chain
  // kept fetching (up to 4 chapters) after Back, competing with the bookmark
  // download/refresh loop that resumes the moment Reader closes. The one
  // fetch already running cannot be cancelled and may still collide with the
  // resumed loop's first item — "newest wins" settles it, and the download
  // retries an interrupted chapter.
  dispose() {
    this.loadGen++;
  }

  // Never rejects: `loading` starts `true` and only `runLoad` clears it, so a
  // throw here used to leave the spinner up forever with no error shown.
  async init(initialChapterNum: number) {
    try {
      const story = await api.getStory(this.storyId);
      if (!story) {
        this.fail("story not found");
        return;
      }
      this.story = story;
      this.chapters = await api.listChapters(this.storyId);
    } catch (e) {
      this.fail(`Could not load story: ${e instanceof Error ? e.message : String(e)}`);
      return;
    }
    await this.loadChapter(initialChapterNum);
  }

  private fail(message: string) {
    this.fetchError = message;
    this.loading = false;
  }

  // A fetch can grow the story's metadata (new chapters on an in-progress
  // fic), and `goToChapter`/prefetch bound on `story.chapterCount` — re-read
  // the row after every fetch so those bounds don't go stale. Only
  // `chapterCount` is merged in: replacing the whole row could clobber a
  // flag toggle that landed while this read was in flight.
  private async refreshStory() {
    const fresh = await api.getStory(this.storyId);
    if (fresh && this.story) this.story = { ...this.story, chapterCount: fresh.chapterCount };
  }

  // Quietly warms the two chapters on either side of wherever you land, so
  // paging back and forth feels instant most of the time. Best-effort and
  // low-priority: skips chapters already cached, bails on the generation
  // check if you navigate again mid-chain, and paces itself the same way
  // the debug tool's bulk prefetch does (a fetch burst with no gaps reads
  // as bot traffic to Cloudflare in a way a single manual fetch doesn't).
  private async prefetchNeighbors(gen: number, centerNum: number) {
    if (!this.story) return;
    const targets = [centerNum - 2, centerNum - 1, centerNum + 1, centerNum + 2].filter(
      (n) => n >= 1 && n <= this.story!.chapterCount
    );
    for (const n of targets) {
      if (gen !== this.loadGen) return;
      let wasFetched = true;
      try {
        const result = await ensureChapterCached(this.storyId, n, { background: true });
        if (result.chapters) this.chapters = result.chapters;
        wasFetched = result.wasFetched;
        if (wasFetched) await this.refreshStory();
      } catch {
        // Best-effort — a failed prefetch just means that chapter fetches
        // normally (foreground) whenever it's actually opened. Treated as
        // an attempt (paces before the next target), not a clean skip.
      }
      if (!wasFetched) continue;
      if (gen !== this.loadGen) return;
      await sleep(jitterDelay());
    }
  }

  // `currentScrollTop`, when given, is stashed under the chapter being
  // navigated *away from* so the caller can restore it on a later visit —
  // this class never touches the DOM, so it takes/returns plain numbers
  // rather than reading `scrollEl` itself.
  //
  // Latest call wins: a load superseded by a newer one still lets its fetch
  // finish (the chapter lands in the DB) but writes no state. A repeat call
  // for the chapter already in flight shares that load instead of fetching
  // twice.
  async loadChapter(num: number, currentScrollTop?: number) {
    if (this.inFlight?.num === num) return this.inFlight.promise;
    const myGen = ++this.loadGen;
    if (currentScrollTop !== undefined) this.scrollPositions.set(this.currentChapterNum, currentScrollTop);
    const entry = { num, promise: this.runLoad(num, myGen) };
    this.inFlight = entry;
    try {
      await entry.promise;
    } finally {
      if (this.inFlight === entry) this.inFlight = undefined;
    }
  }

  private async runLoad(num: number, myGen: number) {
    this.loading = true;
    this.fetchError = "";
    if (!this.story) {
      this.fetchError = "story metadata not loaded yet";
      this.loading = false;
      return;
    }
    let rendered = false;
    try {
      const result = await ensureChapterCached(this.storyId, num);
      if (myGen !== this.loadGen) return;
      if (result.wasFetched) await this.refreshStory();
      if (myGen !== this.loadGen) return;
      if (result.chapters) this.chapters = result.chapters;
      this.content = result.content ?? undefined;
      this.currentChapterNum = num;
      rendered = true;
    } catch (e) {
      if (myGen !== this.loadGen) return;
      this.fetchError = e instanceof Error ? e.message : String(e);
    } finally {
      if (myGen === this.loadGen) this.loading = false;
    }
    if (!rendered) return;

    // The chapter is already on screen: a failure to remember the position or
    // to tell the caller must not replace it with an error banner (the
    // template shows `fetchError` instead of `content`) or skip the prefetch.
    try {
      await api.setResumeChapter(this.storyId, num);
    } catch (e) {
      console.error("could not save resume position:", e);
    }
    try {
      this.onChanged?.();
    } catch (e) {
      console.error("onChanged failed:", e);
    }
    void this.prefetchNeighbors(myGen, num);
  }

  async goToChapter(num: number, currentScrollTop?: number) {
    if (!this.story || num < 1 || num > this.story.chapterCount) return;
    await this.loadChapter(num, currentScrollTop);
  }

  private async toggle(flag: "isRead" | "dismissed" | "bookmarked") {
    if (!this.story) return;
    this.toggleError = "";
    try {
      this.story = await toggleStoryFlag(this.story, flag);
      this.onChanged?.();
    } catch (e) {
      this.toggleError = e instanceof Error ? e.message : String(e);
    }
  }

  toggleRead() {
    return this.toggle("isRead");
  }

  toggleBookmark() {
    return this.toggle("bookmarked");
  }

  toggleDismiss() {
    return this.toggle("dismissed");
  }
}
