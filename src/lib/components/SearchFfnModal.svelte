<script lang="ts">
  // "Search FFN for a new fandom" (SPEC.md §6 discovery). Fetches page 1 of
  // a category+fandom browse listing, server-side sorted by favorites and
  // filtered by word count/status so what lands first is already the most
  // relevant match — see wayfinder decision trail for why (thousands of
  // possible matches; importing everything isn't the goal).
  //
  // Fandom is picked from FFN's own cached category index (exact
  // name/slug), not typed free-text — a typo there is a silent 404, not a
  // graceful error.
  import { X } from "lucide-svelte";
  import * as api from "$lib/api";
  import { fold } from "$lib/normalize";
  import { FFN_CATEGORIES } from "$lib/types";
  import type { FandomIndexEntry, StoryStatus } from "$lib/types";

  let {
    onClose,
    onImported
  }: {
    onClose: () => void;
    onImported: (fandom: string) => void;
  } = $props();

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
    if (!selectedFandom) return;
    searching = true;
    error = "";
    try {
      const result = await api.searchFfnAndWait(selectedFandom.fandom, minWordCount ?? 0, status_, selectedFandom.category);
      onImported(result.fandom);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      searching = false;
    }
  }
</script>

<svelte:window
  onclick={(e) => {
    if (dropdownEl && !dropdownEl.contains(e.target as Node)) dropdownOpen = false;
  }}
/>

<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === "Escape" && onClose()} role="button" tabindex="-1"></div>
<div class="modal" role="dialog" aria-label="Search FFN for a new fandom">
  <div class="modal-head">
    <h2>Search FanFiction.net</h2>
    <button class="close" onclick={onClose} aria-label="Close"><X size={16} /></button>
  </div>

  <form onsubmit={runSearch}>
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
      </div>
    </label>

    <p class="note">Pulls the first page, sorted by favorites — FFN's own ranking, not ours. Use "Load more" afterward for deeper results.</p>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <button type="submit" class="submit" disabled={searching || !selectedFandom}>
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
