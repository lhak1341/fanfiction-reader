<script lang="ts">
  // Discovery entry point for both sources (SPEC.md §6/§8) — fetches page 1
  // of a fandom/tag listing, server-side sorted/filtered so what lands
  // first is already the most relevant match (thousands of possible
  // matches; importing everything isn't the goal). The two sources'
  // fandom-input UIs differ (FFN has a cached category+fandom index; AO3
  // has neither, robots.txt blocks its /autocomplete/ endpoint) — that's
  // deliberately the *only* thing that differs; word-count/status filters
  // and the submit flow are shared.
  import { X } from "lucide-svelte";
  import * as api from "$lib/api";
  import { fold } from "$lib/normalize";
  import { getAo3Tag, saveAo3Tag } from "$lib/fandom";
  import { FFN_CATEGORIES } from "$lib/types";
  import type { FandomIndexEntry, StorySource, StoryStatus } from "$lib/types";

  let {
    onClose,
    onImported
  }: {
    onClose: () => void;
    onImported: (fandom: string, source: StorySource) => void;
  } = $props();

  let source = $state<StorySource>("ffn");

  // ---- FFN fields ----
  const ALL = "all";
  let category = $state<string>(ALL); // "all" = search/browse across every indexed category
  let fandomOptions = $state<FandomIndexEntry[]>([]);
  let fandomQuery = $state("");
  let selectedFandom = $state<FandomIndexEntry | undefined>(undefined);
  let dropdownOpen = $state(false);
  let dropdownEl = $state<HTMLDivElement | undefined>(undefined);
  let indexing = $state(false);
  let indexError = $state("");
  let indexProgress = $state("");

  // ---- AO3 fields ----
  // No cached tag index exists (unlike FFN's fandom_index) — the fandom
  // name is typed free text, and its AO3 tag is looked up from a small
  // user-maintained map (fandom.ts), auto-filled once saved. A typo here
  // fails the same way typing it into AO3's own search would (empty/wrong
  // results), not a silent 404.
  let ao3FandomName = $state("");
  let ao3Tag = $state("");
  $effect(() => {
    const saved = getAo3Tag(ao3FandomName);
    if (saved && !ao3Tag) ao3Tag = saved;
  });

  // ---- Shared fields ----
  let minWordCount = $state<number | undefined>(undefined);
  let status = $state<StoryStatus | undefined>(undefined);
  let status_: StoryStatus | "any" = $derived(status ?? "any");

  let searching = $state(false);
  let error = $state("");

  let filteredOptions = $derived(
    fandomOptions.filter((f) => fold(f.fandom).includes(fold(fandomQuery)))
  );

  function fmtCount(n: number) {
    return n >= 1000 ? `${Math.round(n / 100) / 10}K` : String(n);
  }

  function categoryLabel(value: string) {
    return FFN_CATEGORIES.find((c) => c.value === value)?.label ?? value;
  }

  async function loadFandomOptions() {
    try {
      fandomOptions = await api.listFandomIndex(category === ALL ? undefined : category);
    } catch (e) {
      indexError = e instanceof Error ? e.message : String(e);
    }
  }

  async function refreshIndex(event: Event) {
    event.preventDefault();
    indexing = true;
    indexError = "";
    indexProgress = "";
    try {
      if (category === ALL) {
        await api.indexAllCategoriesAndWait(
          FFN_CATEGORIES.map((c) => c.value),
          (cat, result) => {
            indexProgress = result instanceof Error ? `${categoryLabel(cat)} failed — continuing…` : `Indexed ${categoryLabel(cat)}…`;
          }
        );
      } else {
        await api.indexFfnCategoryAndWait(category);
      }
      await loadFandomOptions();
      indexProgress = "";
    } catch (e) {
      indexError = e instanceof Error ? e.message : String(e);
    } finally {
      indexing = false;
    }
  }

  function selectFandom(f: FandomIndexEntry) {
    selectedFandom = f;
    fandomQuery = f.fandom;
    dropdownOpen = false;
  }

  $effect(() => {
    void category;
    selectedFandom = undefined;
    fandomQuery = "";
    fandomOptions = [];
    loadFandomOptions();
  });

  async function runSearch(event: Event) {
    event.preventDefault();
    searching = true;
    error = "";
    try {
      if (source === "ffn") {
        if (!selectedFandom) return;
        const result = await api.searchFfnAndWait(selectedFandom.fandom, minWordCount ?? 0, status_, selectedFandom.category);
        onImported(result.fandom, "ffn");
      } else {
        if (!ao3FandomName.trim() || !ao3Tag.trim()) return;
        const fandom = ao3FandomName.trim();
        // `fandom` is force-applied to every imported row (searchAo3), so
        // it's already the same label used to unify with FFN's side —
        // unlike a raw AO3 canonical tag name, which could be anything.
        await api.searchAo3AndWait(ao3Tag.trim(), fandom, minWordCount ?? 0, status_);
        saveAo3Tag(fandom, ao3Tag.trim());
        onImported(fandom, "ao3");
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      searching = false;
    }
  }

  let canSubmit = $derived(source === "ffn" ? !!selectedFandom : !!ao3FandomName.trim() && !!ao3Tag.trim());
</script>

<svelte:window
  onclick={(e) => {
    if (dropdownEl && !dropdownEl.contains(e.target as Node)) dropdownOpen = false;
  }}
/>

<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === "Escape" && onClose()} role="button" tabindex="-1"></div>
<div class="modal" role="dialog" aria-label="Search for a new fandom">
  <div class="modal-head">
    <h2>Search</h2>
    <button class="close" onclick={onClose} aria-label="Close"><X size={16} /></button>
  </div>

  <div class="seg source-toggle">
    <button type="button" class:active={source === "ffn"} onclick={() => (source = "ffn")}>FanFiction.net</button>
    <button type="button" class:active={source === "ao3"} onclick={() => (source = "ao3")}>Archive of Our Own</button>
  </div>

  <form onsubmit={runSearch}>
    {#if source === "ffn"}
      <label class="field">
        <span>Category</span>
        <select bind:value={category}>
          <option value={ALL}>All categories (search everything indexed)</option>
          {#each FFN_CATEGORIES as c (c.value)}
            <option value={c.value}>{c.label}</option>
          {/each}
        </select>
      </label>

      <div class="field">
        <span>Fandom</span>
        {#if fandomOptions.length === 0}
          <button class="index-btn" onclick={refreshIndex} disabled={indexing}>
            {indexing ? indexProgress || "Fetching…" : category === ALL ? "Index all categories (one-time, ~9 fetches)" : "Fetch fandom list for this category"}
          </button>
          {#if indexError}<p class="error">{indexError}</p>{/if}
        {:else}
          <div class="fandom-dd" bind:this={dropdownEl}>
            <input
              placeholder="Type to search, or leave blank to browse…"
              bind:value={fandomQuery}
              onfocus={() => (dropdownOpen = true)}
              onclick={() => (dropdownOpen = true)}
            />
            {#if dropdownOpen}
              <ul class="fandom-list">
                {#each filteredOptions.slice(0, 50) as f (`${f.category}:${f.fandom}`)}
                  <li>
                    <button
                      type="button"
                      class:current={selectedFandom?.fandom === f.fandom && selectedFandom?.category === f.category}
                      onclick={() => selectFandom(f)}
                    >
                      <span>{f.fandom}{#if category === ALL}<span class="cat-tag">{categoryLabel(f.category)}</span>{/if}</span>
                      <span class="count tnum">{fmtCount(f.ffnCount)}</span>
                    </button>
                  </li>
                {/each}
                {#if filteredOptions.length === 0}
                  <li class="empty">No matches</li>
                {/if}
              </ul>
            {/if}
          </div>
          <button type="button" class="refresh-link" onclick={refreshIndex} disabled={indexing}>
            {indexing ? indexProgress || "Refreshing…" : category === ALL ? "Re-index all categories" : "Refresh this category"}
          </button>
        {/if}
      </div>
    {:else}
      <label class="field">
        <span>Fandom</span>
        <input placeholder="e.g. Naruto" bind:value={ao3FandomName} />
      </label>

      <label class="field">
        <span>AO3 tag</span>
        <input placeholder="e.g. Naruto (Anime & Manga)" bind:value={ao3Tag} />
      </label>
      <p class="note">
        AO3's exact tag string, not a search term — same as pasting it into AO3's own search
        (e.g. "Harry Potter" is actually tagged "Harry Potter - J.K. Rowling"). Saved after a
        successful search, so you only type it once per fandom.
      </p>
    {/if}

    <label class="field">
      <span>Minimum length</span>
      <div class="seg">
        <button type="button" class:active={minWordCount === undefined} onclick={() => (minWordCount = undefined)}>Any</button>
        <button type="button" class:active={minWordCount === 40000} onclick={() => (minWordCount = 40000)}>40K+</button>
        <button type="button" class:active={minWordCount === 100000} onclick={() => (minWordCount = 100000)}>100K+</button>
      </div>
    </label>

    <label class="field">
      <span>Status</span>
      <div class="seg">
        <button type="button" class:active={status === undefined} onclick={() => (status = undefined)}>Any</button>
        <button type="button" class:active={status === "complete"} onclick={() => (status = "complete")}>Done</button>
        {#if source === "ao3"}
          <button type="button" class:active={status === "in_progress"} onclick={() => (status = "in_progress")}>Ongoing</button>
        {/if}
      </div>
    </label>

    <p class="note">
      {#if source === "ffn"}
        Pulls the first page, sorted by favorites — FFN's own ranking, not ours.
      {:else}
        Pulls the first page, sorted by bookmarks — AO3's closest analog to FFN's favorites.
      {/if}
      Use "Load more" afterward for deeper results.
    </p>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <button type="submit" class="submit" disabled={searching || !canSubmit}>
      {searching ? "Searching…" : "Search"}
    </button>
  </form>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 49;
  }
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 340px;
    background: var(--paper-raised);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: var(--shadow);
    z-index: 50;
    padding: 1rem 1.2rem 1.2rem;
  }
  .modal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.9rem;
  }
  .modal-head h2 {
    font-size: 1.05rem;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    padding: 0.2rem;
  }
  .source-toggle {
    margin-bottom: 0.9rem;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.78rem;
    color: var(--ink-soft);
  }
  .field select,
  .field input {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
    background: var(--paper);
    color: var(--ink);
    font-family: inherit;
  }
  .index-btn {
    border: 1px dashed var(--line);
    border-radius: 6px;
    background: transparent;
    color: var(--ink-soft);
    padding: 0.5rem;
    font-size: 0.78rem;
  }
  .index-btn:disabled {
    opacity: 0.6;
  }
  .fandom-dd {
    position: relative;
  }
  .fandom-list {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    max-height: 220px;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0.3rem;
    background: var(--paper-raised);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
    z-index: 55;
  }
  .fandom-list button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    border: none;
    background: transparent;
    color: var(--ink);
    padding: 0.4rem 0.5rem;
    border-radius: 6px;
    font-size: 0.82rem;
    text-align: left;
  }
  .fandom-list button:hover {
    background: var(--paper);
  }
  .fandom-list button.current {
    background: var(--accent-wash);
    color: var(--accent);
    font-weight: 700;
  }
  .fandom-list .count {
    color: var(--ink-soft);
    font-size: 0.72rem;
  }
  .cat-tag {
    margin-left: 0.4rem;
    font-size: 0.66rem;
    color: var(--accent);
    background: var(--accent-wash);
    border-radius: 4px;
    padding: 0.05rem 0.35rem;
  }
  .fandom-list .empty {
    padding: 0.5rem;
    font-size: 0.78rem;
    color: var(--ink-soft);
  }
  .refresh-link {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--ink-soft);
    font-size: 0.7rem;
    text-decoration: underline;
    padding: 0;
  }
  .refresh-link:disabled {
    opacity: 0.6;
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    background: var(--paper);
    border: none;
    color: var(--ink-soft);
    padding: 0.4rem 0.5rem;
    font-size: 0.78rem;
    border-right: 1px solid var(--line);
  }
  .seg button:last-child {
    border-right: none;
  }
  .seg button.active {
    background: var(--accent-wash);
    color: var(--accent);
    font-weight: 700;
  }
  .note {
    font-size: 0.72rem;
    color: var(--ink-soft);
    line-height: 1.4;
  }
  .error {
    color: var(--rust);
    font-size: 0.78rem;
  }
  .submit {
    border: none;
    background: var(--accent);
    color: var(--paper-raised);
    border-radius: 6px;
    padding: 0.55rem;
    font-size: 0.85rem;
    font-weight: 600;
  }
  .submit:disabled {
    opacity: 0.5;
  }
</style>
