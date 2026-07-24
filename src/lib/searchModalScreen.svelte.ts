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

export class SearchModalScreen {
  source = $state<StorySource>("ffn");

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

  constructor(onImported: (fandom: string, source: StorySource) => void) {
    this.onImported = onImported;
  }

  get canSubmit(): boolean {
    return this.source === "ffn" ? !!this.selectedFandom : !!this.ao3FandomName.trim() && !!this.ao3Tag.trim();
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

  async runSearch() {
    this.searching = true;
    this.error = "";
    try {
      if (this.source === "ffn") {
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
