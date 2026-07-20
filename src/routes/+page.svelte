<script lang="ts">
  import * as api from "$lib/api";
  import { ChevronDown, LayoutGrid, List, RefreshCw } from "lucide-svelte";
  import { ensureChapterCached } from "$lib/chapterCache";
  import { ficSourceAdapter } from "$lib/ficSource";
  import { applyFandomRename } from "$lib/fandom";
  import FandomPicker from "$lib/components/FandomPicker.svelte";
  import Reader from "$lib/components/Reader.svelte";
  import SearchModal from "$lib/components/SearchModal.svelte";
  import StoryList from "$lib/components/StoryList.svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import { getPinnedFandoms, savePinnedFandoms } from "$lib/pinnedFandoms";
  import { getViewMode, saveViewMode } from "$lib/viewMode";
  import { getSearchFilterPrefs, saveSearchFilterPrefs, getBookmarksHideRead, saveBookmarksHideRead } from "$lib/filterPrefs";
  import type { SortBy, StoryRow, StorySource, StoryStatus } from "$lib/types";

  let screen = $state<"search" | "bookmarks" | "reader">("search");
  let lastListScreen = $state<"search" | "bookmarks">("search");
  let readerStoryId = $state<number | undefined>(undefined);
  let readerChapterNum = $state<number | undefined>(undefined);

  function openStory(story: StoryRow) {
    lastListScreen = screen === "reader" ? lastListScreen : screen;
    readerStoryId = story.id;
    readerChapterNum = story.lastChapterNum ?? 1;
    screen = "reader";
  }

  // ---- Search state ----
  let searchFandom = $state<string | undefined>(undefined); // undefined = "All fandoms"
  let searchText = $state("");
  let minWordCount = $state<number | undefined>(undefined);
  let status = $state<StoryStatus | undefined>(undefined);
  let searchSortBy = $state<SortBy>("favorites");
  const initialSearchFilterPrefs = getSearchFilterPrefs();
  let hideRead = $state(initialSearchFilterPrefs.hideRead);
  let hideDismissed = $state(initialSearchFilterPrefs.hideDismissed);
  let hideBookmarked = $state(initialSearchFilterPrefs.hideBookmarked);
  $effect(() => saveSearchFilterPrefs({ hideRead, hideDismissed, hideBookmarked }));
  let pinnedFandoms = $state<Set<string>>(getPinnedFandoms());
  $effect(() => savePinnedFandoms(pinnedFandoms));

  // Shared between Search and Bookmarks — one persisted preference, not
  // per-screen state.
  let viewMode = $state<"list" | "card">(getViewMode());
  $effect(() => saveViewMode(viewMode));

  // `undefined` = All sources. SPEC.md §8: sources aren't merged into one
  // ranking, so this scopes sort-by-favorites/etc to one source at a time.
  let sourceFilter = $state<StorySource | undefined>(undefined);
  // Which source the *last* modal search/import used — "Load more" falls
  // back to this when sourceFilter is "All", since a fandom name alone
  // doesn't say which source to keep paging. Defaults to "ffn" (unchanged
  // behavior for a session that never touches AO3).
  let lastSearchSource = $state<StorySource>("ffn");

  let fandoms = $state<string[]>([]);
  let searchStories = $state<StoryRow[]>([]);
  let searchModalOpen = $state(false);
  let loadingMore = $state(false);
  let loadMoreError = $state("");

  // ---- Bookmarks state ----
  let bookmarkFandom = $state<string | undefined>(undefined);
  let bookmarkSortBy = $state<SortBy>("favorites");
  let bookmarkHideRead = $state(getBookmarksHideRead());
  $effect(() => saveBookmarksHideRead(bookmarkHideRead));

  let bookmarkedFandoms = $state<string[]>([]);
  let bookmarkStories = $state<StoryRow[]>([]);
  let continueReading = $state<StoryRow[]>([]);

  let errorMessage = $state("");

  async function loadFandoms() {
    try {
      fandoms = await api.listFandoms();
    } catch (e) {
      errorMessage = `Could not load fandoms: ${e}`;
    }
  }

  async function loadBookmarkedFandoms() {
    try {
      bookmarkedFandoms = await api.listBookmarkedFandoms();
    } catch (e) {
      errorMessage = `Could not load bookmarked fandoms: ${e}`;
    }
  }

  async function loadSearchStories() {
    try {
      searchStories = await api.listStories({
        fandom: searchFandom,
        source: sourceFilter,
        search: searchText.trim() || undefined,
        minWordCount,
        status,
        sortBy: searchSortBy,
        hideRead,
        hideDismissed,
        hideBookmarked
      });
      errorMessage = "";
    } catch (e) {
      errorMessage = `Could not load stories: ${e}`;
    }
  }

  async function loadBookmarkStories() {
    try {
      bookmarkStories = await api.listBookmarks({
        fandom: bookmarkFandom,
        sortBy: bookmarkSortBy,
        hideRead: bookmarkHideRead
      });
      errorMessage = "";
    } catch (e) {
      errorMessage = `Could not load bookmarks: ${e}`;
    }
  }

  async function loadContinueReading() {
    try {
      continueReading = await api.getContinueReading(5);
    } catch (e) {
      errorMessage = `Could not load continue reading: ${e}`;
    }
  }

  $effect(() => {
    void searchFandom;
    void sourceFilter;
    void searchText;
    void minWordCount;
    void status;
    void searchSortBy;
    void hideRead;
    void hideDismissed;
    void hideBookmarked;
    loadSearchStories();
  });

  $effect(() => {
    void bookmarkFandom;
    void bookmarkSortBy;
    void bookmarkHideRead;
    loadBookmarkStories();
  });

  loadFandoms();
  loadBookmarkedFandoms();
  loadContinueReading();

  // Any toggle can affect both lists (a story visible in Search may be the
  // one just bookmarked/unbookmarked) plus the bookmarked-fandoms scope and
  // the Continue Reading surface — dataset is tiny, so just refresh everything.
  async function refreshAll() {
    await Promise.all([loadSearchStories(), loadBookmarkStories(), loadBookmarkedFandoms(), loadContinueReading()]);
  }

  function sleep(ms: number) {
    return new Promise((r) => setTimeout(r, ms));
  }

  // Re-fetches each in-progress bookmarked story's own page (chapter 1) —
  // that's enough to refresh its metadata (favorites, word/chapter count,
  // the FFN "Updated:" text, and status if it's actually finished now) and
  // pick up any new chapters, same as any other story-page fetch. Skips
  // stories already marked complete locally (SPEC.md §6) — those don't
  // change. Paced the same way as bulk chapter prefetch/discovery: a burst
  // of requests with no gaps reads as bot traffic to Cloudflare.
  let refreshingBookmarks = $state(false);
  let refreshProgress = $state("");

  async function refreshBookmarks() {
    if (refreshingBookmarks) return;
    refreshingBookmarks = true;
    refreshProgress = "";
    try {
      const targets = await api.listBookmarks({ status: "in_progress" });
      for (let i = 0; i < targets.length; i++) {
        const story = targets[i];
        refreshProgress = `Refreshing ${i + 1} of ${targets.length}…`;
        try {
          const url = ficSourceAdapter(story.source).chapterUrl(story, 1, null);
          await api.fetchChapterAndWait(story.source, url, story.id, 1);
        } catch {
          // Best-effort — one story failing (e.g. taken down) shouldn't stop the rest.
        }
        await refreshAll();
        if (i < targets.length - 1) await sleep(1500 + Math.random() * 1500);
      }
    } finally {
      refreshingBookmarks = false;
      refreshProgress = "";
    }
  }

  // Force-fetches every not-yet-cached chapter of one bookmarked story, so
  // it's fully readable offline ahead of time rather than filling in as you
  // navigate (Reader's own prefetch only ever looks 2 chapters ahead). Same
  // paced/best-effort loop as `refreshBookmarks`, scoped to one story at a
  // time (`downloadingStoryId` gates concurrent clicks).
  let downloadingStoryId = $state<number | null>(null);
  let downloadingStoryTitle = $state("");
  let downloadProgress = $state("");
  let downloadDone = $state(0);
  let downloadTotal = $state(0);

  async function downloadAllChapters(story: StoryRow) {
    if (downloadingStoryId !== null) return;
    downloadingStoryId = story.id;
    downloadingStoryTitle = story.title;
    downloadProgress = "";
    downloadDone = 0;
    downloadTotal = 0;
    try {
      let chapters = await api.listChapters(story.id);
      const missing = chapters.filter((c) => !c.isCached);
      downloadTotal = missing.length;
      for (let i = 0; i < missing.length; i++) {
        downloadProgress = `Downloading ${i + 1} of ${missing.length}…`;
        try {
          const result = await ensureChapterCached(story, chapters, story.id, missing[i].chapterNum);
          chapters = result.chapters;
        } catch {
          // Best-effort — one chapter failing shouldn't stop the rest.
        }
        downloadDone = i + 1;
        if (i < missing.length - 1) await sleep(1500 + Math.random() * 1500);
      }
      await refreshAll();
    } finally {
      downloadingStoryId = null;
      downloadProgress = "";
    }
  }

  async function handleImported(importedFandom: string, source: StorySource) {
    searchModalOpen = false;
    lastSearchSource = source;
    await loadFandoms();
    searchFandom = importedFandom;
    await loadSearchStories();
  }

  /// Renames/merges a fandom bucket (FandomPicker's cleanup control) —
  /// cleans up duplicate-looking entries (an AO3 import that kept AO3's own
  /// canonical tag text) or messy crossover buckets (a comma-joined
  /// multi-fandom string). Follows the rename in the current view: keeps
  /// showing the same stories under their new fandom name, and moves a pin
  /// to the new name rather than dropping it.
  async function handleRenameFandom(oldFandom: string, newFandom: string) {
    try {
      await api.renameFandom(oldFandom, newFandom);
      ({ pinnedFandoms, searchFandom } = applyFandomRename(oldFandom, newFandom, pinnedFandoms, searchFandom));
      await loadFandoms();
      await loadSearchStories();
    } catch (e) {
      errorMessage = `Could not rename fandom: ${e}`;
    }
  }

  /// Continues paging the current fandom search. Which source to continue
  /// is `sourceFilter` if the user has explicitly scoped the view to one,
  /// else whichever source the last modal search used (`lastSearchSource`).
  async function loadMore() {
    if (!searchFandom) return;
    loadingMore = true;
    loadMoreError = "";
    try {
      const source = sourceFilter ?? lastSearchSource;
      await ficSourceAdapter(source).loadMore(searchFandom, minWordCount ?? 0, status ?? "any");
      await loadSearchStories();
    } catch (e) {
      loadMoreError = e instanceof Error ? e.message : String(e);
    } finally {
      loadingMore = false;
    }
  }

  async function handleToggleRead(story: StoryRow) {
    try {
      await api.toggleRead(story.id, !story.isRead);
      await refreshAll();
    } catch (e) {
      errorMessage = `Could not update read state: ${e}`;
    }
  }

  async function handleToggleDismissed(story: StoryRow) {
    try {
      await api.toggleDismissed(story.id, !story.dismissed);
      await refreshAll();
    } catch (e) {
      errorMessage = `Could not update dismissed state: ${e}`;
    }
  }

  async function handleToggleBookmarked(story: StoryRow) {
    try {
      await api.toggleBookmarked(story.id, !story.bookmarked);
      await refreshAll();
    } catch (e) {
      errorMessage = `Could not update bookmark: ${e}`;
    }
  }

  function fmtChapter(story: StoryRow) {
    return story.lastChapterNum ? `Ch. ${story.lastChapterNum} of ${story.chapterCount}` : "Not started";
  }
</script>

{#if screen === "reader" && readerStoryId !== undefined && readerChapterNum !== undefined}
  <Reader
    storyId={readerStoryId}
    initialChapterNum={readerChapterNum}
    onBack={() => (screen = lastListScreen)}
    onChanged={refreshAll}
  />
{:else}
<div class="app">
  <div class="top-shell">
    <div class="wordmark">FanFiction<em>Reader</em></div>
    <div class="screen-tabs" role="tablist">
      <button role="tab" aria-selected={screen === "search"} onclick={() => (screen = "search")}>Search</button>
      <button role="tab" aria-selected={screen === "bookmarks"} onclick={() => (screen = "bookmarks")}>Bookmarks</button>
    </div>
    <div class="top-shell-right">
      <button
        class="refresh-btn"
        class:spinning={refreshingBookmarks}
        onclick={refreshBookmarks}
        disabled={refreshingBookmarks}
        title={refreshingBookmarks ? refreshProgress : "Refresh bookmarked stories (in-progress only)"}
      >
        <RefreshCw size={15} />
      </button>
      <ThemeToggle />
      <a class="debug-link" href="/debug">Debug tools</a>
    </div>
  </div>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  {#if downloadingStoryId !== null}
    <div class="download-banner">
      <span class="download-banner-text">Downloading "{downloadingStoryTitle}" — {downloadProgress}</span>
      <div class="download-banner-track">
        <div class="download-banner-fill" style="width: {downloadTotal ? (downloadDone / downloadTotal) * 100 : 0}%"></div>
      </div>
    </div>
  {/if}

  {#if screen === "search"}
    <div class="filterbar">
      <div class="filterbar-row">
        <FandomPicker
          fandoms={fandoms}
          selected={searchFandom}
          pinnable
          bind:pinnedFandoms
          onSelect={(f) => (searchFandom = f)}
          onRename={handleRenameFandom}
        />
        <button class="search-ffn-btn" onclick={() => (searchModalOpen = true)}>Search</button>

        <input class="search-input" placeholder="Search title, author, or summary…" bind:value={searchText} />

        <div class="sort-select-wrap">
          <select class="sort-select" bind:value={searchSortBy}>
            <option value="favorites">Sort: Favorites</option>
            <option value="updated">Sort: Updated</option>
            <option value="words">Sort: Word count</option>
          </select>
          <ChevronDown size={13} class="sort-caret" />
        </div>

        <div class="view-toggle" role="group" aria-label="View">
          <button class:active={viewMode === "list"} onclick={() => (viewMode = "list")} title="List view"><List size={15} /></button>
          <button class:active={viewMode === "card"} onclick={() => (viewMode = "card")} title="Card view"><LayoutGrid size={15} /></button>
        </div>
      </div>

      <div class="filterbar-row">
        <div class="field-group" role="group" aria-label="Source">
          <button class:active={sourceFilter === undefined} onclick={() => (sourceFilter = undefined)}>All</button>
          <button class:active={sourceFilter === "ffn"} onclick={() => (sourceFilter = "ffn")}>FFN</button>
          <button class:active={sourceFilter === "ao3"} onclick={() => (sourceFilter = "ao3")}>AO3</button>
        </div>

        <div class="field-group" role="group" aria-label="Minimum word count">
          <button class:active={minWordCount === undefined} onclick={() => (minWordCount = undefined)}>Any length</button>
          <button class:active={minWordCount === 40000} onclick={() => (minWordCount = 40000)}>40K+</button>
          <button class:active={minWordCount === 100000} onclick={() => (minWordCount = 100000)}>100K+</button>
        </div>

        <div class="field-group" role="group" aria-label="Status">
          <button class:active={status === undefined} onclick={() => (status = undefined)}>Any status</button>
          <button class:active={status === "complete"} onclick={() => (status = "complete")}>Done</button>
          <button class:active={status === "in_progress"} onclick={() => (status = "in_progress")}>Ongoing</button>
        </div>

        <label class="hide-toggle">
          <input type="checkbox" bind:checked={hideRead} /> Hide read
        </label>
        <label class="hide-toggle">
          <input type="checkbox" bind:checked={hideDismissed} /> Hide dismissed
        </label>
        <label class="hide-toggle">
          <input type="checkbox" bind:checked={hideBookmarked} /> Hide bookmarked
        </label>
      </div>
    </div>

    {#if searchStories.length === 0}
      {#if searchFandom}
        <p class="empty-note">
          {#if fandoms.includes(searchFandom)}
            You've gone through everything imported for "{searchFandom}" with these filters.
          {:else}
            No stories for "{searchFandom}" yet.
          {/if}
          <br />
          <button class="load-more-btn" onclick={loadMore} disabled={loadingMore}>
            {loadingMore ? "Loading…" : "Load more"}
          </button>
          {#if loadMoreError}<span class="error">{loadMoreError}</span>{/if}
        </p>
      {:else}
        <p class="empty-note">
          No stories yet. Click <button class="link-btn" onclick={() => (searchModalOpen = true)}>Search</button> to pull some in.
        </p>
      {/if}
    {:else}
      <StoryList
        stories={searchStories}
        viewMode={viewMode}
        onOpenStory={openStory}
        onToggleRead={handleToggleRead}
        onToggleDismissed={handleToggleDismissed}
        onToggleBookmarked={handleToggleBookmarked}
      />
      {#if searchFandom}
        <div class="load-more-row">
          <button class="load-more-btn" onclick={loadMore} disabled={loadingMore}>
            {loadingMore ? "Loading…" : "Load more"}
          </button>
          {#if loadMoreError}<span class="error">{loadMoreError}</span>{/if}
        </div>
      {/if}
    {/if}
  {:else}
    {#if continueReading.length > 0}
      <div class="continue-reading">
        <h3>Continue Reading</h3>
        <div class="continue-row">
          {#each continueReading as story (story.id)}
            <button class="continue-card" onclick={() => openStory(story)}>
              <div class="t">{story.title}</div>
              <div class="meta">{story.fandom} · {fmtChapter(story)}</div>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="filterbar">
      <div class="filterbar-row">
        <FandomPicker fandoms={bookmarkedFandoms} selected={bookmarkFandom} onSelect={(f) => (bookmarkFandom = f)} />

        <div class="sort-select-wrap">
          <select class="sort-select" bind:value={bookmarkSortBy}>
            <option value="favorites">Sort: Favorites</option>
            <option value="updated">Sort: Updated</option>
            <option value="words">Sort: Word count</option>
          </select>
          <ChevronDown size={13} class="sort-caret" />
        </div>

        <label class="hide-toggle">
          <input type="checkbox" bind:checked={bookmarkHideRead} /> Hide read
        </label>

        <div class="view-toggle" role="group" aria-label="View">
          <button class:active={viewMode === "list"} onclick={() => (viewMode = "list")} title="List view"><List size={15} /></button>
          <button class:active={viewMode === "card"} onclick={() => (viewMode = "card")} title="Card view"><LayoutGrid size={15} /></button>
        </div>
      </div>
    </div>

    {#if bookmarkStories.length === 0}
      <p class="empty-note">No bookmarks yet. Bookmark a story from Search to see it here.</p>
    {:else}
      <StoryList
        stories={bookmarkStories}
        viewMode={viewMode}
        showFandom
        includeDismiss={false}
        onOpenStory={openStory}
        onToggleRead={handleToggleRead}
        onToggleDismissed={handleToggleDismissed}
        onToggleBookmarked={handleToggleBookmarked}
        onDownloadAll={downloadAllChapters}
        downloadingStoryId={downloadingStoryId}
        downloadProgress={downloadDone && downloadTotal ? `Downloading ${downloadDone} of ${downloadTotal}…` : downloadProgress}
      />
    {/if}
  {/if}
</div>
{#if searchModalOpen}
  <SearchModal onClose={() => (searchModalOpen = false)} onImported={handleImported} />
{/if}
{/if}

<style>
  .app {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .top-shell {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.85rem 1.5rem;
    border-bottom: 1px solid var(--line);
    background: var(--paper-raised);
  }
  .wordmark {
    font-family: var(--serif);
    font-size: 1.15rem;
  }
  .wordmark em {
    font-style: italic;
    color: var(--accent);
  }
  .top-shell-right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 0.9rem;
  }
  .debug-link {
    font-size: 0.72rem;
    color: var(--ink-soft);
    text-decoration: none;
  }
  .debug-link:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .refresh-btn {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--paper);
    color: var(--ink-soft);
  }
  .refresh-btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .refresh-btn:disabled {
    opacity: 0.6;
  }
  .refresh-btn.spinning :global(svg) {
    animation: refresh-spin 0.9s linear infinite;
  }
  @keyframes refresh-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .screen-tabs {
    display: inline-flex;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 3px;
    gap: 2px;
  }
  .screen-tabs button {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    font-size: 0.82rem;
    padding: 0.4rem 0.9rem;
    border-radius: 999px;
    font-weight: 600;
  }
  .screen-tabs button[aria-selected="true"] {
    background: var(--accent);
    color: var(--paper-raised);
  }

  .filterbar {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0.9rem 1.5rem;
    border-bottom: 1px solid var(--line);
    background: var(--paper-raised);
  }
  .filterbar-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.6rem;
  }

  .search-input,
  .sort-select {
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.45rem 0.7rem;
    background: var(--paper);
    color: var(--ink);
    font-size: 0.85rem;
  }
  .search-input {
    flex: 1;
    min-width: 160px;
  }

  .sort-select-wrap {
    position: relative;
    display: inline-flex;
  }
  .sort-select {
    appearance: none;
    -webkit-appearance: none;
    font: inherit;
    font-size: 0.78rem;
    padding-right: 1.7rem;
    cursor: pointer;
  }
  :global(.sort-caret) {
    position: absolute;
    right: 0.6rem;
    top: 50%;
    transform: translateY(-50%);
    color: var(--ink-soft);
    pointer-events: none;
  }

  .field-group {
    display: inline-flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .field-group button {
    background: var(--paper);
    border: none;
    color: var(--ink-soft);
    padding: 0.4rem 0.7rem;
    font-size: 0.78rem;
    border-right: 1px solid var(--line);
  }
  .field-group button:last-child {
    border-right: none;
  }
  .field-group button.active {
    background: var(--accent-wash);
    color: var(--accent);
    font-weight: 700;
  }

  .hide-toggle {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.78rem;
    color: var(--ink-soft);
  }

  .view-toggle {
    display: inline-flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
    margin-left: auto;
  }
  .view-toggle button {
    display: grid;
    place-items: center;
    background: var(--paper);
    border: none;
    width: 32px;
    height: 32px;
    color: var(--ink-soft);
    border-right: 1px solid var(--line);
  }
  .view-toggle button:last-child {
    border-right: none;
  }
  .view-toggle button.active {
    background: var(--accent-wash);
    color: var(--accent);
  }

  .error {
    color: var(--rust);
    padding: 0.8rem 1.5rem 0;
  }
  .download-banner {
    padding: 0.6rem 1.5rem 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .download-banner-text {
    font-size: 0.78rem;
    color: var(--ink-soft);
  }
  .download-banner-track {
    height: 4px;
    border-radius: 999px;
    background: var(--accent-wash);
    overflow: hidden;
  }
  .download-banner-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s ease;
  }
  .empty-note {
    padding: 2.5rem 1.5rem;
    text-align: center;
    color: var(--ink-soft);
  }

  .search-ffn-btn {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--paper);
    color: var(--ink);
    padding: 0.45rem 0.8rem;
    font-size: 0.82rem;
    font-weight: 600;
  }
  .search-ffn-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .link-btn {
    border: none;
    background: transparent;
    color: var(--accent);
    font: inherit;
    text-decoration: underline;
    padding: 0;
  }

  .load-more-row {
    display: flex;
    justify-content: center;
    padding: 1rem;
    border-top: 1px solid var(--line);
  }
  .load-more-btn {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--paper-raised);
    color: var(--ink);
    padding: 0.5rem 1rem;
    font-size: 0.82rem;
    font-weight: 600;
  }
  .load-more-btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .load-more-btn:disabled {
    opacity: 0.5;
  }

  .continue-reading {
    padding: 1rem 1.5rem 0.4rem;
    border-bottom: 1px solid var(--line);
    background: var(--paper-raised);
  }
  .continue-reading h3 {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ink-soft);
    font-family: var(--sans);
    margin-bottom: 0.6rem;
  }
  .continue-row {
    display: flex;
    gap: 0.7rem;
    overflow-x: auto;
    padding-bottom: 1rem;
  }
  .continue-card {
    flex-shrink: 0;
    width: 220px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--paper);
    padding: 0.7rem 0.9rem;
    text-align: left;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .continue-card:hover {
    border-color: var(--accent);
  }
  .continue-card .t {
    font-family: var(--serif);
    font-size: 0.92rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .continue-card .meta {
    font-size: 0.74rem;
    color: var(--ink-soft);
    margin-top: 0.2rem;
  }
</style>
