// Bookmarks screen state + logic, split out of +page.svelte (architecture
// review candidate 3, 2026-07-21) — same rationale as searchScreen.svelte.ts.
// refresh()/downloadAllChapters() only reload this screen's own data
// (reloadAll()); +page.svelte's callers additionally reload SearchScreen
// afterward, since a story visible in both screens needs both refreshed —
// that composition is +page.svelte's job, not this class's (see
// architecture review candidate 3 grill, "cross-cutting logic" decision).

import * as api from "./api";
import { ensureChapterCached } from "./chapterCache";
import { describeFailures, PauseGate, runPacedBestEffort } from "./pacedLoop";
import type { SortBy, StoryRow } from "./types";

// Failed download chapters get this many extra passes (pacing: pacedLoop.ts).
const DOWNLOAD_RETRY_ROUNDS = 2;

export class BookmarksScreen {
  fandom = $state<string | undefined>(undefined);
  sortBy = $state<SortBy>("favorites");
  hideRead = $state(false);

  fandoms = $state<string[]>([]);
  stories = $state<StoryRow[]>([]);
  continueReading = $state<StoryRow[]>([]);

  // Architecture review candidate 4 (2026-07-21): the single value
  // +page.svelte's $effect watches to trigger loadStories() — see
  // SearchScreen.reloadTrigger for why this is a plain getter, not $derived.
  get reloadTrigger() {
    return [this.fandom, this.sortBy, this.hideRead];
  }

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

  // The bulk loops below keep running when +page.svelte swaps Reader in (this
  // instance lives in the page script, not in the list screens), where they
  // would compete with Reader's own fetches for the one FFN webview. The page
  // pauses them while Reader is shown; `refreshing`/`downloadingStoryId` stay
  // set meanwhile, so their buttons stay disabled.
  private gate = new PauseGate();

  constructor(onError: (message: string) => void, initialHideRead: boolean) {
    this.onError = onError;
    this.hideRead = initialHideRead;
  }

  pauseBackgroundFetches() {
    this.gate.pause();
  }

  resumeBackgroundFetches() {
    this.gate.resume();
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
    if (this.refreshing || this.downloadingStoryId !== null) return;
    this.refreshing = true;
    this.refreshProgress = "";
    try {
      const targets = await api.listBookmarks({ status: "in_progress" });
      // Best-effort — one story failing (e.g. taken down) shouldn't stop the
      // rest. No retry rounds: a removed story fails every time.
      const { failed } = await runPacedBestEffort(targets, (story) => api.fetchChapterAndWait(story.id, 1, true), {
        onProgress: ({ index, total }) => (this.refreshProgress = `Refreshing ${index + 1} of ${total}…`),
        gate: this.gate
      });
      await this.reloadAll();
      if (failed.length > 0) {
        this.onError(
          `${failed.length} ${failed.length === 1 ? "story" : "stories"} failed to refresh: ${describeFailures(failed)}`
        );
      }
    } finally {
      this.refreshing = false;
      this.refreshProgress = "";
    }
  }

  async downloadAllChapters(story: StoryRow) {
    if (this.downloadingStoryId !== null) return;
    if (this.refreshing) {
      this.onError("Wait for the bookmark refresh to finish before downloading — both use the one fetch window.");
      return;
    }
    this.downloadingStoryId = story.id;
    this.downloadingStoryTitle = story.title;
    this.downloadProgress = "";
    this.downloadDone = 0;
    this.downloadTotal = 0;
    try {
      const chapters = await api.listChapters(story.id);
      const missing = chapters.filter((c) => !c.isCached).map((c) => c.chapterNum);
      this.downloadTotal = missing.length;

      // A failed chapter (Cloudflare page, interrupted, timeout, empty
      // result) is retried after a longer pause rather than left as a hole.
      let firstPassFailures = 0;
      const { failed } = await runPacedBestEffort(
        missing,
        async (num) => {
          const result = await ensureChapterCached(story.id, num, { background: true });
          if (!result.content?.isCached) throw new Error("fetched, but the chapter came back empty");
        },
        {
          retryRounds: DOWNLOAD_RETRY_ROUNDS,
          gate: this.gate,
          onProgress: ({ round, index, total, result }) => {
            if (!(result instanceof Error)) this.downloadDone++;
            else if (round === 0) firstPassFailures++;
            this.downloadProgress =
              round === 0
                ? `Downloading ${index + 1} of ${total}${firstPassFailures ? ` (${firstPassFailures} failed)` : ""}…`
                : `Retry ${index + 1} of ${total} (round ${round} of ${DOWNLOAD_RETRY_ROUNDS})…`;
          }
        }
      );
      await this.reloadAll();
      if (failed.length > 0) {
        this.onError(
          `Download of "${story.title}" incomplete: ${failed.length} chapter${failed.length === 1 ? "" : "s"} failed ` +
            `${describeFailures(failed, (n) => n)}. Click Download again to retry.`
        );
      }
    } finally {
      this.downloadingStoryId = null;
      this.downloadProgress = "";
    }
  }
}
