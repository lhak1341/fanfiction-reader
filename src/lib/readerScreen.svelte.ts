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

  // Bumped on every loadChapter call; a running prefetch chain checks it
  // before each step and bails the moment a real navigation supersedes it.
  private prefetchGen = 0;

  private readonly storyId: number;
  private readonly onChanged?: () => void;

  constructor(storyId: number, onChanged?: () => void) {
    this.storyId = storyId;
    this.onChanged = onChanged;
  }

  async init(initialChapterNum: number) {
    this.story = (await api.getStory(this.storyId)) ?? undefined;
    this.chapters = await api.listChapters(this.storyId);
    await this.loadChapter(initialChapterNum);
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
      if (gen !== this.prefetchGen) return;
      let wasFetched = true;
      try {
        const result = await ensureChapterCached(this.story, this.chapters, this.storyId, n);
        this.chapters = result.chapters;
        wasFetched = result.wasFetched;
      } catch {
        // Best-effort — a failed prefetch just means that chapter fetches
        // normally (foreground) whenever it's actually opened. Treated as
        // an attempt (paces before the next target), not a clean skip.
      }
      if (!wasFetched) continue;
      if (gen !== this.prefetchGen) return;
      await sleep(jitterDelay());
    }
  }

  // `currentScrollTop`, when given, is stashed under the chapter being
  // navigated *away from* so the caller can restore it on a later visit —
  // this class never touches the DOM, so it takes/returns plain numbers
  // rather than reading `scrollEl` itself.
  async loadChapter(num: number, currentScrollTop?: number) {
    const myGen = ++this.prefetchGen;
    if (currentScrollTop !== undefined) this.scrollPositions.set(this.currentChapterNum, currentScrollTop);
    this.loading = true;
    this.fetchError = "";
    if (!this.story) {
      this.fetchError = "story metadata not loaded yet";
      this.loading = false;
      return;
    }
    try {
      const result = await ensureChapterCached(this.story, this.chapters, this.storyId, num);
      this.chapters = result.chapters;
      this.content = result.content ?? undefined;
      this.currentChapterNum = num;
      await api.setResumeChapter(this.storyId, num);
      this.onChanged?.();
      void this.prefetchNeighbors(myGen, num);
    } catch (e) {
      this.fetchError = e instanceof Error ? e.message : String(e);
    } finally {
      this.loading = false;
    }
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
