<script lang="ts">
  // Discovery entry point for both sources (SPEC.md §6/§8) — fetches page 1
  // of a fandom/tag listing, server-side sorted/filtered so what lands
  // first is already the most relevant match (thousands of possible
  // matches; importing everything isn't the goal). The two sources'
  // fandom-input UIs differ (FFN has a cached category+fandom index; AO3
  // has neither, robots.txt blocks its /autocomplete/ endpoint) — that's
  // deliberately the *only* thing that differs; word-count/status filters
  // and the submit flow are shared.
  import { onDestroy } from "svelte";
  import { X } from "lucide-svelte";
  import { STATUS_LABELS, WORD_COUNT_PRESETS } from "$lib/filterOptions";
  import { fmtCompact } from "$lib/format";
  import { ALL, SearchModalScreen } from "$lib/searchModalScreen.svelte";
  import { FFN_CATEGORIES } from "$lib/types";
  import type { FandomIndexEntry, StorySource } from "$lib/types";

  let {
    onClose,
    onImported,
    onStoryAdded
  }: {
    onClose: () => void;
    onImported: (fandom: string, source: StorySource) => void;
    // "By URL" bookmarks the story it adds, then calls this (not `onImported`).
    onStoryAdded: () => void;
  } = $props();

  // +page.svelte mounts this behind `{#if search.modalOpen}`, so it always
  // fully remounts per open — onImported never changes during one mount's
  // lifetime.
  // svelte-ignore state_referenced_locally
  const screen = new SearchModalScreen(onImported, onStoryAdded);

  let dropdownOpen = $state(false);
  let dropdownEl = $state<HTMLDivElement | undefined>(undefined);

  // No cached tag index exists (unlike FFN's fandom_index) — the fandom
  // name is typed free text, and its AO3 tag is looked up from a small
  // user-maintained map (fandom.ts) by the screen's autofill rule. A typo
  // here fails the same way typing it into AO3's own search would
  // (empty/wrong results), not a silent 404.
  screen.start();
  onDestroy(() => screen.dispose());

  function categoryLabel(value: string) {
    return FFN_CATEGORIES.find((c) => c.value === value)?.label ?? value;
  }

  function selectFandom(f: FandomIndexEntry) {
    screen.selectFandom(f);
    dropdownOpen = false;
  }
</script>

<svelte:window
  onclick={(e) => {
    if (dropdownEl && !dropdownEl.contains(e.target as Node)) dropdownOpen = false;
  }}
/>

<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === "Escape" && onClose()} role="button" tabindex="-1"></div>
<div class="modal" role="dialog" aria-label="Search for a new fandom or add a story by URL">
  <div class="modal-head">
    <h2>Search</h2>
    <button class="close" onclick={onClose} aria-label="Close"><X size={16} /></button>
  </div>

  <div class="seg source-toggle">
    <button type="button" class:active={screen.mode === "ffn"} onclick={() => (screen.mode = "ffn")}>FanFiction.net</button>
    <button type="button" class:active={screen.mode === "ao3"} onclick={() => (screen.mode = "ao3")}>Archive of Our Own</button>
    <button type="button" class:active={screen.mode === "url"} onclick={() => (screen.mode = "url")}>By URL</button>
  </div>

  <form onsubmit={(e) => { e.preventDefault(); screen.runSearch(); }}>
    {#if screen.mode === "ffn"}
      <label class="field">
        <span>Category</span>
        <select bind:value={screen.category}>
          <option value={ALL}>All categories (search everything indexed)</option>
          {#each FFN_CATEGORIES as c (c.value)}
            <option value={c.value}>{c.label}</option>
          {/each}
        </select>
      </label>

      <div class="field">
        <span>Fandom</span>
        {#if screen.fandomOptions.length === 0}
          <button class="index-btn" onclick={(e) => { e.preventDefault(); screen.refreshIndex(); }} disabled={screen.indexing}>
            {screen.indexing ? screen.indexProgress || "Fetching…" : screen.category === ALL ? "Index all categories (one-time, ~9 fetches)" : "Fetch fandom list for this category"}
          </button>
          {#if screen.indexError}<p class="error">{screen.indexError}</p>{/if}
        {:else}
          <div class="fandom-dd" bind:this={dropdownEl}>
            <input
              placeholder="Type to search, or leave blank to browse…"
              bind:value={screen.fandomQuery}
              onfocus={() => (dropdownOpen = true)}
              onclick={() => (dropdownOpen = true)}
            />
            {#if dropdownOpen}
              <ul class="fandom-list">
                {#each screen.filteredOptions.slice(0, 50) as f (`${f.category}:${f.fandom}`)}
                  <li>
                    <button
                      type="button"
                      class:current={screen.selectedFandom?.fandom === f.fandom && screen.selectedFandom?.category === f.category}
                      onclick={() => selectFandom(f)}
                    >
                      <span>{f.fandom}{#if screen.category === ALL}<span class="cat-tag">{categoryLabel(f.category)}</span>{/if}</span>
                      <span class="count tnum">{fmtCompact(f.ffnCount)}</span>
                    </button>
                  </li>
                {/each}
                {#if screen.filteredOptions.length === 0}
                  <li class="empty">No matches</li>
                {/if}
              </ul>
            {/if}
          </div>
          <button type="button" class="refresh-link" onclick={() => screen.refreshIndex()} disabled={screen.indexing}>
            {screen.indexing ? screen.indexProgress || "Refreshing…" : screen.category === ALL ? "Re-index all categories" : "Refresh this category"}
          </button>
        {/if}
      </div>
    {:else if screen.mode === "ao3"}
      <label class="field">
        <span>Fandom</span>
        <input placeholder="e.g. Naruto" bind:value={screen.ao3FandomName} />
      </label>

      <label class="field">
        <span>AO3 tag</span>
        <input placeholder="e.g. Naruto (Anime & Manga)" bind:value={screen.ao3Tag} />
      </label>
      <p class="note">
        AO3's exact tag string, not a search term — same as pasting it into AO3's own search
        (e.g. "Harry Potter" is actually tagged "Harry Potter - J.K. Rowling"). Saved after a
        successful search, so you only type it once per fandom.
      </p>
    {:else}
      <label class="field">
        <span>Story URL</span>
        <input placeholder="https://www.fanfiction.net/s/14583034/1/…" bind:value={screen.url} />
      </label>
      <p class="note">
        A FanFiction.net story/chapter link or an AO3 work link. Adds that one story, with its full
        chapter list, to your Bookmarks.
      </p>
    {/if}
    {#if screen.mode !== "url"}
    <label class="field">
      <span>Minimum length</span>
      <div class="seg">
        <button type="button" class:active={screen.minWordCount === undefined} onclick={() => (screen.minWordCount = undefined)}>Any</button>
        {#each WORD_COUNT_PRESETS as preset}
          <button type="button" class:active={screen.minWordCount === preset.value} onclick={() => (screen.minWordCount = preset.value)}>{preset.label}</button>
        {/each}
      </div>
    </label>

    <label class="field">
      <span>Status</span>
      <div class="seg">
        <button type="button" class:active={screen.status === undefined} onclick={() => (screen.status = undefined)}>Any</button>
        {#each screen.statusOptions as status}
          <button type="button" class:active={screen.status === status} onclick={() => (screen.status = status)}>{STATUS_LABELS[status]}</button>
        {/each}
      </div>
    </label>

    <p class="note">
      {#if screen.mode === "ffn"}
        Pulls the first page, sorted by favorites — FFN's own ranking, not ours.
      {:else}
        Pulls the first page, sorted by bookmarks — AO3's closest analog to FFN's favorites.
      {/if}
      Use "Load more" afterward for deeper results.
    </p>

    {/if}
    {#if screen.error}
      <p class="error">{screen.error}</p>
    {/if}

    <button
      type="submit"
      class="submit"
      disabled={screen.searching || !screen.canSubmit}
    >
      {screen.searching ? (screen.mode === "url" ? "Adding…" : "Searching…") : screen.mode === "url" ? "Add story" : "Search"}
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
