// SearchModal's async orchestration, split out of SearchModal.svelte
// (architecture review candidate 8, 2026-07-24) so it's reachable from
// Vitest — same rationale and shape as searchScreen.svelte.ts/
// bookmarksScreen.svelte.ts/readerScreen.svelte.ts. Dropdown/DOM-coupled UI
// state (dropdownOpen, dropdownEl, fandomQuery, filteredOptions,
// selectFandom) and both $effects (ao3 tag autofill, category-change reset)
// stay in the component — no async in them, and $effect needs a live
// component/effect-root context a bare `new SearchModalScreen()` doesn't
// have. Errors stay local to the modal (no onError callback) — unlike
// SearchScreen/ReaderScreen, nothing here needs to reach +page.svelte's
// shared error banner; the modal already shows its own inline error and
// just closes on success.

import * as api from "./api";
import { saveAo3Tag } from "./fandom";
import { FFN_CATEGORIES } from "./types";
import type { FandomIndexEntry, StorySource, StoryStatus } from "./types";

export const ALL = "all";

export type SearchMode = StorySource | "url";

export class SearchModalScreen {
  mode = $state<SearchMode>("ffn");

  // "By URL" mode: one pasted story/chapter URL, source auto-detected
  // server-side (`add_story_by_url`).
  url = $state("");

  category = $state<string>(ALL);
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

  private onImported: (fandom: string, source: StorySource) => void;
  private onStoryAdded: () => void;

  /// `onStoryAdded` fires after a "By URL" import has bookmarked the story —
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

  async loadFandomOptions() {
    try {
      this.fandomOptions = await api.listFandomIndex(this.category === ALL ? undefined : this.category);
    } catch (e) {
      this.indexError = e instanceof Error ? e.message : String(e);
    }
  }

  async refreshIndex() {
    this.indexing = true;
    this.indexError = "";
    this.indexProgress = "";
    try {
      if (this.category === ALL) {
        await api.indexAllCategoriesAndWait(
          FFN_CATEGORIES.map((c) => c.value),
          (cat, result) => {
            const label = FFN_CATEGORIES.find((c) => c.value === cat)?.label ?? cat;
            this.indexProgress = result instanceof Error ? `${label} failed — continuing…` : `Indexed ${label}…`;
          }
        );
      } else {
        await api.indexFfnCategoryAndWait(this.category);
      }
      await this.loadFandomOptions();
      this.indexProgress = "";
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
        const { storyId } = await api.addStoryByUrl(this.url.trim());
        try {
          await api.toggleBookmarked(storyId, true);
        } catch (e) {
          // The import itself succeeded — say so, and stay in the modal so the
          // message isn't lost (onStoryAdded would close it and jump to
          // Bookmarks, where this story isn't listed).
          this.error = `Imported, but couldn't bookmark it: ${e instanceof Error ? e.message : String(e)}`;
          return;
        }
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
