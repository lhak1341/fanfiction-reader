// Search screen state + logic, split out of +page.svelte (architecture
// review candidate 3, 2026-07-21) so it's reachable from Vitest — a plain
// .svelte.ts class instead of a .svelte route. Reactive auto-reload and
// pref persistence live in `start()` (reopened 2026-10-08): it supplies its
// own $effect.root, and vitest runs client-compiled runes
// (vitest.client-env.ts), so a bare `new SearchScreen()` can run them.

import * as api from "./api";
import type { ErrorBanner } from "./errorBanner.svelte";
import { applyFandomRename, renameAo3Tag } from "./fandom";
import { ficSourceAdapter } from "./ficSource";
import { getSearchFilterPrefs, saveSearchFilterPrefs } from "./filterPrefs";
import { getPinnedFandoms, savePinnedFandoms } from "./pinnedFandoms";
import type { SortBy, StoryRow, StorySource, StoryStatus } from "./types";

export class SearchScreen {
  fandom = $state<string | undefined>(undefined); // undefined = "All fandoms"
  text = $state("");
  minWordCount = $state<number | undefined>(undefined);
  status = $state<StoryStatus | undefined>(undefined);
  sortBy = $state<SortBy>("favorites");
  hideRead = $state(false);
  hideDismissed = $state(false);
  hideBookmarked = $state(false);
  pinnedFandoms = $state<Set<string>>(new Set());

  // `undefined` = All sources. SPEC.md §8: sources aren't merged into one
  // ranking, so this scopes sort-by-favorites/etc to one source at a time.
  sourceFilter = $state<StorySource | undefined>(undefined);
  // Which source the *last* modal search/import used — "Load more" falls
  // back to this when sourceFilter is "All", since a fandom name alone
  // doesn't say which source to keep paging. Defaults to "ffn" (unchanged
  // behavior for a session that never touches AO3).
  lastSearchSource = $state<StorySource>("ffn");

  fandoms = $state<string[]>([]);
  stories = $state<StoryRow[]>([]);
  modalOpen = $state(false);

  // Architecture review candidate 4 (2026-07-21): the one place that lists
  // which fields affect the story list; `start()`'s reload effect watches it.
  // $effect tracks whatever $state fields it reads, so reading this getter
  // inside the effect is enough.
  private get reloadTrigger() {
    return [
      this.fandom,
      this.sourceFilter,
      this.text,
      this.minWordCount,
      this.status,
      this.sortBy,
      this.hideRead,
      this.hideDismissed,
      this.hideBookmarked,
    ];
  }
  loadingMore = $state(false);
  loadMoreError = $state("");

  private banner: ErrorBanner;
  private stop: (() => void) | undefined;

  constructor(banner: ErrorBanner) {
    this.banner = banner;
    const prefs = getSearchFilterPrefs();
    this.hideRead = prefs.hideRead;
    this.hideDismissed = prefs.hideDismissed;
    this.hideBookmarked = prefs.hideBookmarked;
    this.pinnedFandoms = getPinnedFandoms();
  }

  /// Starts persisting the filter prefs and pinned fandoms, and reloading the
  /// list whenever a filter changes (the first run is the initial load).
  /// Idempotent; `dispose()` stops it.
  start() {
    if (this.stop) return;
    this.stop = $effect.root(() => {
      $effect(() =>
        saveSearchFilterPrefs({
          hideRead: this.hideRead,
          hideDismissed: this.hideDismissed,
          hideBookmarked: this.hideBookmarked,
        })
      );
      $effect(() => savePinnedFandoms(this.pinnedFandoms));
      $effect(() => {
        void this.reloadTrigger;
        this.loadStories();
      });
    });
  }

  dispose() {
    this.stop?.();
    this.stop = undefined;
  }

  async loadFandoms() {
    await this.banner.run("search.fandoms", "Could not load fandoms", async () => {
      this.fandoms = await api.listFandoms();
    });
  }

  async loadStories() {
    await this.banner.run("search.stories", "Could not load stories", async () => {
      this.stories = await api.listStories({
        fandom: this.fandom,
        source: this.sourceFilter,
        search: this.text.trim() || undefined,
        minWordCount: this.minWordCount,
        status: this.status,
        sortBy: this.sortBy,
        hideRead: this.hideRead,
        hideDismissed: this.hideDismissed,
        hideBookmarked: this.hideBookmarked,
      });
    });
  }

  /// Continues paging the current fandom search. Which source to continue
  /// is `sourceFilter` if the user has explicitly scoped the view to one,
  /// else whichever source the last modal search used (`lastSearchSource`).
  async loadMore() {
    if (!this.fandom) return;
    this.loadingMore = true;
    this.loadMoreError = "";
    try {
      const source = this.sourceFilter ?? this.lastSearchSource;
      await ficSourceAdapter(source).loadMore(
        this.fandom,
        this.minWordCount ?? 0,
        this.status ?? "any"
      );
      await this.loadStories();
    } catch (e) {
      this.loadMoreError = e instanceof Error ? e.message : String(e);
    } finally {
      this.loadingMore = false;
    }
  }

  async handleImported(importedFandom: string, source: StorySource) {
    this.modalOpen = false;
    this.lastSearchSource = source;
    await this.loadFandoms();
    this.fandom = importedFandom;
    await this.loadStories();
  }

  /// Renames/merges a fandom bucket (FandomPicker's cleanup control) —
  /// cleans up duplicate-looking entries (an AO3 import that kept AO3's own
  /// canonical tag text) or messy crossover buckets (a comma-joined
  /// multi-fandom string). Follows the rename in the current view: keeps
  /// showing the same stories under their new fandom name, and moves a pin
  /// to the new name rather than dropping it.
  async renameFandom(oldFandom: string, newFandom: string) {
    await this.banner.run("search.rename", "Could not rename fandom", async () => {
      await api.renameFandom(oldFandom, newFandom);
      renameAo3Tag(oldFandom, newFandom);
      ({ pinnedFandoms: this.pinnedFandoms, searchFandom: this.fandom } = applyFandomRename(
        oldFandom,
        newFandom,
        this.pinnedFandoms,
        this.fandom
      ));
      await this.loadFandoms();
      await this.loadStories();
    });
  }
}
