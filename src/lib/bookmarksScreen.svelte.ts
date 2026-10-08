// Bookmarks screen state + logic, split out of +page.svelte (architecture
// review candidate 3, 2026-07-21) — same rationale as searchScreen.svelte.ts.
// refresh()/downloadAllChapters() only reload this screen's own data
// (reloadAll()); +page.svelte's callers additionally reload SearchScreen
// afterward, since a story visible in both screens needs both refreshed —
// that composition is +page.svelte's job, not this class's (see
// architecture review candidate 3 grill, "cross-cutting logic" decision).

import * as api from "./api";
import type { ErrorBanner } from "./errorBanner.svelte";
import { describeFailures, PauseGate, runPacedBestEffort } from "./pacedLoop";
import type { SortBy, StoryRow } from "./types";

// Failed download chapters get this many extra passes (pacing: pacedLoop.ts).
const DOWNLOAD_RETRY_ROUNDS = 2;

export type BulkJob = { kind: "refresh" } | { kind: "download"; storyId: number; title: string };

/// `label` is what the banner, the row tooltip and the refresh button title
/// show; `done`/`total` drive the download bar.
export interface BulkProgress {
  label: string;
  done: number;
  total: number;
}

const NO_PROGRESS: BulkProgress = { label: "", done: 0, total: 0 };

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

  // The one bulk job running, if any — refresh and Download-all are mutually
  // exclusive (one FFN webview; paced like bulk chapter prefetch/discovery, since
  // a burst with no gaps reads as bot traffic to Cloudflare). `busy` is the
  // single "is a job running" question every button and guard asks.
  // Download-all force-fetches every not-yet-cached chapter of one bookmarked
  // story so it's fully readable offline, rather than filling in as you
  // navigate (Reader's own prefetch only ever looks 2 chapters ahead).
  job = $state<BulkJob | null>(null);
  progress = $state<BulkProgress>(NO_PROGRESS);

  get busy() {
    return this.job !== null;
  }

  private banner: ErrorBanner;

  // The bulk loops below keep running when +page.svelte swaps Reader in (this
  // instance lives in the page script, not in the list screens), where they
  // would compete with Reader's own fetches for the one FFN webview. The page
  // pauses them while Reader is shown; `job` stays set meanwhile, so the
  // buttons stay disabled.
  private gate = new PauseGate();

  constructor(banner: ErrorBanner, initialHideRead: boolean) {
    this.banner = banner;
    this.hideRead = initialHideRead;
  }

  pauseBackgroundFetches() {
    this.gate.pause();
  }

  resumeBackgroundFetches() {
    this.gate.resume();
  }

  async loadFandoms() {
    await this.banner.run("bookmarks.fandoms", "Could not load bookmarked fandoms", async () => {
      this.fandoms = await api.listBookmarkedFandoms();
    });
  }

  async loadStories() {
    await this.banner.run("bookmarks.list", "Could not load bookmarks", async () => {
      this.stories = await api.listBookmarks({ fandom: this.fandom, sortBy: this.sortBy, hideRead: this.hideRead });
    });
  }

  async loadContinueReading() {
    await this.banner.run("bookmarks.continue", "Could not load continue reading", async () => {
      this.continueReading = await api.getContinueReading(5);
    });
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
    if (this.job) return;
    this.job = { kind: "refresh" };
    this.banner.dismiss("bookmarks.refresh");
    this.progress = NO_PROGRESS;
    try {
      const targets = await api.listBookmarks({ status: "in_progress" });
      // Best-effort — one story failing (e.g. taken down) shouldn't stop the
      // rest. No retry rounds: a removed story fails every time.
      const { failed } = await runPacedBestEffort(targets, (story) => api.fetchChapterAndWait(story.id, 1, true), {
        onProgress: ({ index, total }) =>
          (this.progress = { label: `Refreshing ${index + 1} of ${total}…`, done: index, total }),
        gate: this.gate
      });
      await this.reloadAll();
      if (failed.length > 0) {
        this.banner.report(
          "bookmarks.refresh",
          `${failed.length} ${failed.length === 1 ? "story" : "stories"} failed to refresh: ${describeFailures(failed)}`
        );
      }
    } finally {
      this.job = null;
      this.progress = NO_PROGRESS;
    }
  }

  async downloadAllChapters(story: StoryRow) {
    if (this.job) return;
    this.job = { kind: "download", storyId: story.id, title: story.title };
    this.banner.dismiss("bookmarks.download");
    this.progress = NO_PROGRESS;
    try {
      const chapters = await api.listChapters(story.id);
      const missing = chapters.filter((c) => !c.isCached).map((c) => c.chapterNum);
      this.progress = { label: "", done: 0, total: missing.length };

      // A failed chapter (Cloudflare page, interrupted, timeout, empty
      // page) is retried after a longer pause rather than left as a hole.
      let firstPassFailures = 0;
      let done = 0;
      const { failed } = await runPacedBestEffort(
        missing,
        async (num) => {
          // Resolves only once the chapter is cached — an empty page rejects.
          await api.ensureChapter(story.id, num, true);
        },
        {
          retryRounds: DOWNLOAD_RETRY_ROUNDS,
          gate: this.gate,
          onProgress: ({ round, index, total, result }) => {
            if (!(result instanceof Error)) done++;
            else if (round === 0) firstPassFailures++;
            const label =
              round === 0
                ? `Downloading ${index + 1} of ${total}${firstPassFailures ? ` (${firstPassFailures} failed)` : ""}…`
                : `Retry ${index + 1} of ${total} (round ${round} of ${DOWNLOAD_RETRY_ROUNDS})…`;
            this.progress = { label, done, total: missing.length };
          }
        }
      );
      await this.reloadAll();
      if (failed.length > 0) {
        this.banner.report(
          "bookmarks.download",
          `Download of "${story.title}" incomplete: ${failed.length} chapter${failed.length === 1 ? "" : "s"} failed ` +
            `${describeFailures(failed, (n) => n)}. Click Download again to retry.`
        );
      }
    } finally {
      this.job = null;
      this.progress = NO_PROGRESS;
    }
  }
}
