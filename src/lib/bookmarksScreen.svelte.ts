// Bookmarks screen state + logic, split out of +page.svelte (architecture
// review candidate 3, 2026-07-21) — same rationale as searchScreen.svelte.ts.
// refresh()/downloadAllChapters() only reload this screen's own data
// (reloadAll()); +page.svelte's callers additionally reload SearchScreen
// afterward, since a story visible in both screens needs both refreshed —
// that composition is +page.svelte's job, not this class's (see
// architecture review candidate 3 grill, "cross-cutting logic" decision).

import * as api from "./api";
import { ensureChapterCached } from "./chapterCache";
import { ficSourceAdapter } from "./ficSource";
import { runPacedBestEffort } from "./pacedLoop";
import type { SortBy, StoryRow } from "./types";

export class BookmarksScreen {
  fandom = $state<string | undefined>(undefined);
  sortBy = $state<SortBy>("favorites");
  hideRead = $state(false);

  fandoms = $state<string[]>([]);
  stories = $state<StoryRow[]>([]);
  continueReading = $state<StoryRow[]>([]);

  // Paced the same way as bulk chapter prefetch/discovery: a burst of
  // requests with no gaps reads as bot traffic to Cloudflare.
  refreshing = $state(false);
  refreshProgress = $state("");

  // Force-fetches every not-yet-cached chapter of one bookmarked story, so
  // it's fully readable offline ahead of time rather than filling in as you
  // navigate (Reader's own prefetch only ever looks 2 chapters ahead).
  // Scoped to one story at a time (`downloadingStoryId` gates concurrent
  // clicks).
  downloadingStoryId = $state<number | null>(null);
  downloadingStoryTitle = $state("");
  downloadProgress = $state("");
  downloadDone = $state(0);
  downloadTotal = $state(0);

  private onError: (message: string) => void;

  constructor(onError: (message: string) => void, initialHideRead: boolean) {
    this.onError = onError;
    this.hideRead = initialHideRead;
  }

  async loadFandoms() {
    try {
      this.fandoms = await api.listBookmarkedFandoms();
    } catch (e) {
      this.onError(`Could not load bookmarked fandoms: ${e}`);
    }
  }

  async loadStories() {
    try {
      this.stories = await api.listBookmarks({ fandom: this.fandom, sortBy: this.sortBy, hideRead: this.hideRead });
      this.onError("");
    } catch (e) {
      this.onError(`Could not load bookmarks: ${e}`);
    }
  }

  async loadContinueReading() {
    try {
      this.continueReading = await api.getContinueReading(5);
    } catch (e) {
      this.onError(`Could not load continue reading: ${e}`);
    }
  }

  async reloadAll() {
    await Promise.all([this.loadStories(), this.loadFandoms(), this.loadContinueReading()]);
  }

  // Re-fetches each in-progress bookmarked story's own page (chapter 1) —
  // that's enough to refresh its metadata (favorites, word/chapter count,
  // the FFN "Updated:" text, and status if it's actually finished now) and
  // pick up any new chapters, same as any other story-page fetch. Skips
  // stories already marked complete locally (SPEC.md §6) — those don't
  // change.
  async refresh() {
    if (this.refreshing) return;
    this.refreshing = true;
    this.refreshProgress = "";
    try {
      const targets = await api.listBookmarks({ status: "in_progress" });
      // Best-effort — one story failing (e.g. taken down) shouldn't stop the
      // rest; runPacedBestEffort catches and keeps going regardless.
      await runPacedBestEffort(
        targets,
        (story) => {
          const url = ficSourceAdapter(story.source).chapterUrl(story, 1, null);
          return api.fetchChapterAndWait(story.source, url, story.id, 1);
        },
        (i, total) => (this.refreshProgress = `Refreshing ${i + 1} of ${total}…`)
      );
      await this.reloadAll();
    } finally {
      this.refreshing = false;
      this.refreshProgress = "";
    }
  }

  async downloadAllChapters(story: StoryRow) {
    if (this.downloadingStoryId !== null) return;
    this.downloadingStoryId = story.id;
    this.downloadingStoryTitle = story.title;
    this.downloadProgress = "";
    this.downloadDone = 0;
    this.downloadTotal = 0;
    try {
      let chapters = await api.listChapters(story.id);
      const missing = chapters.filter((c) => !c.isCached);
      this.downloadTotal = missing.length;
      await runPacedBestEffort(
        missing,
        async (target) => {
          const result = await ensureChapterCached(story, chapters, story.id, target.chapterNum);
          chapters = result.chapters;
          // Best-effort — one chapter failing shouldn't stop the rest,
          // runPacedBestEffort catches and keeps going regardless.
        },
        (i) => {
          this.downloadProgress = `Downloading ${i + 1} of ${missing.length}…`;
          this.downloadDone = i + 1;
        }
      );
      await this.reloadAll();
    } finally {
      this.downloadingStoryId = null;
      this.downloadProgress = "";
    }
  }
}
