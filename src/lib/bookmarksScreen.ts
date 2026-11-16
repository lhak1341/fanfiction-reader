// Bookmarks screen state + logic, split out of +page.svelte (architecture
// review candidate 3, 2026-07-21) — same rationale as searchScreen.ts.
// refresh()/downloadAllChapters() only reload this screen's own data
// (reloadAll()); the app's callers (`src/app/App.tsx`) additionally reload SearchScreen
// afterward, since a story visible in both screens needs both refreshed —
// that composition is the app's job, not this class's (see
// architecture review candidate 3 grill, "cross-cutting logic" decision).

import * as api from "./api";
import type { ErrorBanner } from "./errorBanner";
import { saveBookmarksHideRead } from "./filterPrefs";
import { describeFailures, PauseGate, runPacedBestEffort } from "./pacedLoop";
import type { ChapterListItem, SortBy, StoryRow } from "./types";
import { Store } from "./store";

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

/// The chapters Download-all fetches: every chapter row not yet cached, plus
/// every number up to `chapterCount` with no row at all. Rows come only from a
/// fetched story page's TOC, so a story bookmarked from Search and never
/// opened has none, and a listing import can raise `chapterCount` past the
/// TOC — going by rows alone, either one downloaded nothing and said nothing.
export function chaptersToDownload(
  chapters: readonly ChapterListItem[],
  chapterCount: number
): number[] {
  const cached = chapters.filter((c) => c.isCached).map((c) => c.chapterNum);
  const last = Math.max(chapterCount, ...chapters.map((c) => c.chapterNum));
  const missing: number[] = [];
  for (let n = 1; n <= last; n++) if (!cached.includes(n)) missing.push(n);
  return missing;
}

export class BookmarksScreen extends Store {
  fandom: string | undefined = undefined;
  sortBy: SortBy = "favorites";
  hideRead = false;

  fandoms: string[] = [];
  stories: StoryRow[] = [];
  continueReading: StoryRow[] = [];

  // Architecture review candidate 4 (2026-07-21): the one place that lists
  // which fields affect the story list; `start()`'s reload reaction watches it.
  private static readonly RELOAD_TRIGGER = [
    "fandom",
    "sortBy",
    "hideRead",
  ] as const satisfies readonly (keyof BookmarksScreen)[];

  // The one bulk job running, if any — refresh and Download-all are mutually
  // exclusive (one FFN webview; paced like bulk chapter prefetch/discovery, since
  // a burst with no gaps reads as bot traffic to Cloudflare). `busy` is the
  // single "is a job running" question every button and guard asks.
  // Download-all force-fetches every not-yet-cached chapter of one bookmarked
  // story so it's fully readable offline, rather than filling in as you
  // navigate (Reader's own prefetch only ever looks 2 chapters ahead).
  job: BulkJob | null = null;
  progress: BulkProgress = NO_PROGRESS;

  get busy() {
    return this.job !== null;
  }

  private banner: ErrorBanner;
  private stop: (() => void) | undefined;

  // The bulk loops below keep running when the app swaps Reader in (this
  // instance lives in the page script, not in the list screens), where they
  // would compete with Reader's own fetches for the one FFN webview. The page
  // pauses them while Reader is shown; `job` stays set meanwhile, so the
  // buttons stay disabled.
  private gate = new PauseGate();

  constructor(banner: ErrorBanner, initialHideRead: boolean) {
    super();
    this.banner = banner;
    this.hideRead = initialHideRead;
  }

  /// Starts persisting `hideRead` and reloading the list whenever a filter
  /// changes (the first run is the initial load). Idempotent; `dispose()`
  /// stops it.
  start() {
    if (this.stop) return;
    const stops = [
      this.watch(["hideRead"], () => saveBookmarksHideRead(this.hideRead)),
      this.watch(BookmarksScreen.RELOAD_TRIGGER, () => void this.loadStories()),
    ];
    this.stop = () => stops.forEach((stop) => stop());
  }

  dispose() {
    this.stop?.();
    this.stop = undefined;
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
      this.stories = await api.listBookmarks({
        fandom: this.fandom,
        sortBy: this.sortBy,
        hideRead: this.hideRead,
      });
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
  //
  // Architecture review candidate 5 (2026-10-09), rejected: a Rust
  // `refresh_story` would be `fetch_chapter(id, 1, true)` renamed — the same
  // chapter-1 page is fetched either way — and this pacing/pause/progress
  // loop would stay here, so it fails the deletion test.
  async refresh() {
    if (this.job) return;
    this.job = { kind: "refresh" };
    this.banner.dismiss("bookmarks.refresh");
    this.progress = NO_PROGRESS;
    try {
      const targets = await api.listBookmarks({ status: "in_progress" });
      // Best-effort — one story failing (e.g. taken down) shouldn't stop the
      // rest. No retry rounds: a removed story fails every time.
      const { failed } = await runPacedBestEffort(
        targets,
        (story) => api.fetchChapterAndWait(story.id, 1, true),
        {
          onProgress: ({ index, total }) =>
            (this.progress = { label: `Refreshing ${index + 1} of ${total}…`, done: index, total }),
          gate: this.gate,
        }
      );
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
      const missing = chaptersToDownload(chapters, story.chapterCount);
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
          },
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
