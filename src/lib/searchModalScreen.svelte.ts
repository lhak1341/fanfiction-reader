// SearchModal's state, async orchestration and reactive rules, split out of
// SearchModal.svelte (architecture review candidate 8, 2026-07-24) so it's
// reachable from Vitest — same rationale and shape as searchScreen.svelte.ts/
// bookmarksScreen.svelte.ts/readerScreen.svelte.ts. Only DOM-coupled UI
// state (dropdownOpen, dropdownEl) stays in the component.
//
// Reopened 2026-10-08: the two $effects (AO3 tag autofill, category-change
// reset) used to stay in the component on the premise that $effect needs a
// live component/effect-root context. `start()` supplies one ($effect.root,
// stopped by `dispose()`), and vitest runs client-compiled runes
// (vitest.client-env.ts), so the "fandom name decides the tag" rule and the
// category reset are tested here.
//
// Errors stay local to the modal (no onError callback) — unlike
// SearchScreen/ReaderScreen, nothing here needs to reach +page.svelte's
// shared error banner; the modal already shows its own inline error and
// just closes on success.

import { untrack } from "svelte";
import * as api from "./api";
import { getAo3Tag, saveAo3Tag } from "./fandom";
import { SEARCH_STATUSES } from "./filterOptions";
import { fold } from "./normalize";
import { describeFailures, runPacedBestEffort } from "./pacedLoop";
import { FFN_CATEGORIES } from "./types";
import type { FandomIndexEntry, StorySource, StoryStatus } from "./types";

export const ALL = "all";

// One retry pass for a category page that failed (pacing: pacedLoop.ts).
const INDEX_RETRY_ROUNDS = 1;

export type SearchMode = StorySource | "url";

export class SearchModalScreen {
  mode = $state<SearchMode>("ffn");

  // "By URL" mode: one pasted story/chapter URL, source auto-detected
  // server-side (`add_story_by_url`).
  url = $state("");

  category = $state<string>(ALL);
  // The FFN fandom picker's text box; `filteredOptions` narrows by it.
  fandomQuery = $state("");
  fandomOptions = $state<FandomIndexEntry[]>([]);
  selectedFandom = $state<FandomIndexEntry | undefined>(undefined);
  indexing = $state(false);
  indexError = $state("");
  indexProgress = $state("");

  ao3FandomName = $state("");
  ao3Tag = $state("");

  minWordCount = $state<number | undefined>(undefined);
  status = $state<StoryStatus | undefined>(undefined);

  searching = $state(false);
  error = $state("");

  // The tag `syncAo3Tag` last filled in, so a later rename can tell an
  // autofilled tag (replaceable) from one the user typed (never touched).
  private autofilledTag: string | undefined;
  // Bumped per `loadFandomOptions` and on `dispose()`; a response for a
  // superseded category is dropped.
  private optionsGen = 0;
  private stop: (() => void) | undefined;

  private onImported: (fandom: string, source: StorySource) => void;
  private onStoryAdded: () => void;

  /// `onStoryAdded` fires after a "By URL" import (which bookmarks the story) —
  /// a pasted URL is deliberate intent to keep that one story, so it lands in
  /// Bookmarks rather than at the bottom of its fandom's Search list.
  constructor(onImported: (fandom: string, source: StorySource) => void, onStoryAdded: () => void = () => {}) {
    this.onImported = onImported;
    this.onStoryAdded = onStoryAdded;
  }

  get canSubmit(): boolean {
    if (this.mode === "url") return !!this.url.trim();
    return this.mode === "ffn" ? !!this.selectedFandom : !!this.ao3FandomName.trim() && !!this.ao3Tag.trim();
  }

  /// Statuses the current mode's Source can narrow a new search by (none for
  /// "By URL", which ignores the filters). Switching to a Source that doesn't offer the selected status
  /// clears it (`start()`), so a hidden choice never reaches `runSearch`.
  get statusOptions(): StoryStatus[] {
    return this.mode === "url" ? [] : SEARCH_STATUSES[this.mode];
  }

  get filteredOptions(): FandomIndexEntry[] {
    const q = fold(this.fandomQuery);
    return this.fandomOptions.filter((f) => fold(f.fandom).includes(q));
  }

  selectFandom(f: FandomIndexEntry) {
    this.selectedFandom = f;
    this.fandomQuery = f.fandom;
  }

  /// Starts the reactive rules: AO3 tag autofill, the status clamp and the
  /// category-change reset. Idempotent; `dispose()` stops them.
  start() {
    if (this.stop) return;
    this.stop = $effect.root(() => {
      $effect(() => this.syncAo3Tag());
      $effect(() => {
        if (this.mode !== "url" && this.status && !this.statusOptions.includes(this.status)) this.status = undefined;
      });
      $effect(() => {
        void this.category;
        untrack(() => this.resetForCategory());
      });
    });
  }

  dispose() {
    this.stop?.();
    this.stop = undefined;
    this.optionsGen++;
  }

  /// A fandom name decides its AO3 tag: fill in the saved tag while the tag
  /// box is empty or still holds what we last filled in. A tag the user typed
  /// is never replaced; a name with no saved tag clears an autofilled one.
  syncAo3Tag() {
    if (this.ao3Tag && this.ao3Tag !== this.autofilledTag) return;
    const saved = getAo3Tag(this.ao3FandomName);
    this.ao3Tag = saved ?? "";
    this.autofilledTag = saved;
  }

  private resetForCategory() {
    this.selectedFandom = undefined;
    this.fandomQuery = "";
    this.fandomOptions = [];
    this.loadFandomOptions();
  }

  async loadFandomOptions() {
    const gen = ++this.optionsGen;
    try {
      const options = await api.listFandomIndex(this.category === ALL ? undefined : this.category);
      if (gen === this.optionsGen) this.fandomOptions = options;
    } catch (e) {
      if (gen === this.optionsGen) this.indexError = e instanceof Error ? e.message : String(e);
    }
  }

  async refreshIndex() {
    this.indexing = true;
    this.indexError = "";
    this.indexProgress = "";
    let failedIndexing = "";
    try {
      if (this.category === ALL) {
        const label = (cat: string) => FFN_CATEGORIES.find((c) => c.value === cat)?.label ?? cat;
        const { failed } = await runPacedBestEffort(
          FFN_CATEGORIES.map((c) => c.value),
          (cat) => api.indexFfnCategoryAndWait(cat),
          {
            retryRounds: INDEX_RETRY_ROUNDS,
            onProgress: ({ item, result }) => {
              this.indexProgress = result instanceof Error ? `${label(item)} failed — continuing…` : `Indexed ${label(item)}…`;
            }
          }
        );
        failedIndexing = failed.length > 0 ? `${failed.length} categories failed to index ${describeFailures(failed, label)}` : "";
      } else {
        await api.indexFfnCategoryAndWait(this.category);
      }
      await this.loadFandomOptions();
      this.indexProgress = "";
      if (failedIndexing) this.indexError = failedIndexing;
    } catch (e) {
      this.indexError = e instanceof Error ? e.message : String(e);
    } finally {
      this.indexing = false;
    }
  }

  /// Name kept from the search modes; also runs the "By URL" import.
  async runSearch() {
    this.searching = true;
    this.error = "";
    try {
      if (this.mode === "url") {
        if (!this.url.trim()) return;
        // The command bookmarks the story it imports (`add_story_by_url`).
        await api.addStoryByUrl(this.url.trim());
        this.onStoryAdded();
      } else if (this.mode === "ffn") {
        if (!this.selectedFandom) return;
        const result = await api.searchFfnAndWait(
          this.selectedFandom.fandom,
          this.minWordCount ?? 0,
          this.status ?? "any",
          this.selectedFandom.category
        );
        this.onImported(result.fandom, "ffn");
      } else {
        if (!this.ao3FandomName.trim() || !this.ao3Tag.trim()) return;
        const fandom = this.ao3FandomName.trim();
        await api.searchAo3AndWait(this.ao3Tag.trim(), fandom, this.minWordCount ?? 0, this.status ?? "any");
        saveAo3Tag(fandom, this.ao3Tag.trim());
        this.onImported(fandom, "ao3");
      }
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.searching = false;
    }
  }
}
