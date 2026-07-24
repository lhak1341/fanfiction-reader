<script lang="ts">
  import { ChevronDown, LayoutGrid, List, RefreshCw } from "lucide-svelte";
  import FandomPicker from "$lib/components/FandomPicker.svelte";
  import Reader from "$lib/components/Reader.svelte";
  import SearchModal from "$lib/components/SearchModal.svelte";
  import StoryList from "$lib/components/StoryList.svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import { SearchScreen } from "$lib/searchScreen.svelte";
  import { BookmarksScreen } from "$lib/bookmarksScreen.svelte";
  import { toggleReadAndRefresh, toggleDismissedAndRefresh, toggleBookmarkedAndRefresh } from "$lib/storyFlagActions";
  import { getViewMode, saveViewMode } from "$lib/viewMode";
  import { saveSearchFilterPrefs, getBookmarksHideRead, saveBookmarksHideRead } from "$lib/filterPrefs";
  import { savePinnedFandoms } from "$lib/pinnedFandoms";
  import type { StoryRow } from "$lib/types";

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

  let errorMessage = $state("");

  const search = new SearchScreen((msg) => (errorMessage = msg));
  const bookmarks = new BookmarksScreen((msg) => (errorMessage = msg), getBookmarksHideRead());

  // Shared between Search and Bookmarks — one persisted preference, not
  // per-screen state.
  let viewMode = $state<"list" | "card">(getViewMode());
  $effect(() => saveViewMode(viewMode));

  // Reactive auto-reload and pref persistence live here rather than inside
  // SearchScreen/BookmarksScreen's constructors — `$effect` needs a live
  // component/effect-root context, which the classes don't have when
  // constructed directly in a unit test (see searchScreen.svelte.ts).
  $effect(() => saveSearchFilterPrefs({ hideRead: search.hideRead, hideDismissed: search.hideDismissed, hideBookmarked: search.hideBookmarked }));
  $effect(() => savePinnedFandoms(search.pinnedFandoms));
  $effect(() => {
    void search.reloadTrigger;
    search.loadStories();
  });

  $effect(() => saveBookmarksHideRead(bookmarks.hideRead));
  $effect(() => {
    void bookmarks.reloadTrigger;
    bookmarks.loadStories();
  });

  search.loadFandoms();
  bookmarks.loadFandoms();
  bookmarks.loadContinueReading();

  // Any toggle can affect both lists (a story visible in Search may be the
  // one just bookmarked/unbookmarked) plus the bookmarked-fandoms scope and
  // the Continue Reading surface — dataset is tiny, so just refresh everything.
  async function refreshAll() {
    await Promise.all([search.loadStories(), bookmarks.reloadAll()]);
  }

  async function handleRefreshBookmarks() {
    await bookmarks.refresh();
    await search.loadStories();
  }

  async function handleDownloadAllChapters(story: StoryRow) {
    await bookmarks.downloadAllChapters(story);
    await search.loadStories();
  }

  function fmtChapter(story: StoryRow) {
    return story.lastChapterNum ? `Ch. ${story.lastChapterNum} of ${story.chapterCount}` : "Not started";
  }

  function setError(msg: string) {
    errorMessage = msg;
  }

  const handleToggleRead = (story: StoryRow) => toggleReadAndRefresh(story, setError, refreshAll);
  const handleToggleDismissed = (story: StoryRow) => toggleDismissedAndRefresh(story, setError, refreshAll);
  const handleToggleBookmarked = (story: StoryRow) => toggleBookmarkedAndRefresh(story, setError, refreshAll);
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
        class:spinning={bookmarks.refreshing}
        onclick={handleRefreshBookmarks}
        disabled={bookmarks.refreshing}
        title={bookmarks.refreshing ? bookmarks.refreshProgress : "Refresh bookmarked stories (in-progress only)"}
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

  {#if bookmarks.downloadingStoryId !== null}
    <div class="download-banner">
      <span class="download-banner-text">Downloading "{bookmarks.downloadingStoryTitle}" — {bookmarks.downloadProgress}</span>
      <div class="download-banner-track">
        <div class="download-banner-fill" style="width: {bookmarks.downloadTotal ? (bookmarks.downloadDone / bookmarks.downloadTotal) * 100 : 0}%"></div>
      </div>
    </div>
  {/if}

  {#if screen === "search"}
    <div class="filterbar">
      <div class="filterbar-row">
        <FandomPicker
          fandoms={search.fandoms}
          selected={search.fandom}
          pinnable
          bind:pinnedFandoms={search.pinnedFandoms}
          onSelect={(f) => (search.fandom = f)}
          onRename={(oldFandom, newFandom) => search.renameFandom(oldFandom, newFandom)}
        />
        <button class="search-ffn-btn" onclick={() => (search.modalOpen = true)}>Search</button>

        <input class="search-input" placeholder="Search title, author, or summary…" bind:value={search.text} />

        <div class="sort-select-wrap">
          <select class="sort-select" bind:value={search.sortBy}>
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
          <button class:active={search.sourceFilter === undefined} onclick={() => (search.sourceFilter = undefined)}>All</button>
          <button class:active={search.sourceFilter === "ffn"} onclick={() => (search.sourceFilter = "ffn")}>FFN</button>
          <button class:active={search.sourceFilter === "ao3"} onclick={() => (search.sourceFilter = "ao3")}>AO3</button>
        </div>

        <div class="field-group" role="group" aria-label="Minimum word count">
          <button class:active={search.minWordCount === undefined} onclick={() => (search.minWordCount = undefined)}>Any length</button>
          <button class:active={search.minWordCount === 40000} onclick={() => (search.minWordCount = 40000)}>40K+</button>
          <button class:active={search.minWordCount === 100000} onclick={() => (search.minWordCount = 100000)}>100K+</button>
        </div>

        <div class="field-group" role="group" aria-label="Status">
          <button class:active={search.status === undefined} onclick={() => (search.status = undefined)}>Any status</button>
          <button class:active={search.status === "complete"} onclick={() => (search.status = "complete")}>Done</button>
          <button class:active={search.status === "in_progress"} onclick={() => (search.status = "in_progress")}>Ongoing</button>
        </div>

        <label class="hide-toggle">
          <input type="checkbox" bind:checked={search.hideRead} /> Hide read
        </label>
        <label class="hide-toggle">
          <input type="checkbox" bind:checked={search.hideDismissed} /> Hide dismissed
        </label>
        <label class="hide-toggle">
          <input type="checkbox" bind:checked={search.hideBookmarked} /> Hide bookmarked
        </label>
      </div>
    </div>

    {#if search.stories.length === 0}
      {#if search.fandom}
        <p class="empty-note">
          {#if search.fandoms.includes(search.fandom)}
            You've gone through everything imported for "{search.fandom}" with these filters.
          {:else}
            No stories for "{search.fandom}" yet.
          {/if}
          <br />
          <button class="load-more-btn" onclick={() => search.loadMore()} disabled={search.loadingMore}>
            {search.loadingMore ? "Loading…" : "Load more"}
          </button>
          {#if search.loadMoreError}<span class="error">{search.loadMoreError}</span>{/if}
        </p>
      {:else}
        <p class="empty-note">
          No stories yet. Click <button class="link-btn" onclick={() => (search.modalOpen = true)}>Search</button> to pull some in.
        </p>
      {/if}
    {:else}
      <StoryList
        stories={search.stories}
        viewMode={viewMode}
        onOpenStory={openStory}
        onToggleRead={handleToggleRead}
        onToggleDismissed={handleToggleDismissed}
        onToggleBookmarked={handleToggleBookmarked}
      />
      {#if search.fandom}
        <div class="load-more-row">
          <button class="load-more-btn" onclick={() => search.loadMore()} disabled={search.loadingMore}>
            {search.loadingMore ? "Loading…" : "Load more"}
          </button>
          {#if search.loadMoreError}<span class="error">{search.loadMoreError}</span>{/if}
        </div>
      {/if}
    {/if}
  {:else}
    {#if bookmarks.continueReading.length > 0}
      <div class="continue-reading">
        <h3>Continue Reading</h3>
        <div class="continue-row">
          {#each bookmarks.continueReading as story (story.id)}
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
        <FandomPicker fandoms={bookmarks.fandoms} selected={bookmarks.fandom} onSelect={(f) => (bookmarks.fandom = f)} />

        <div class="sort-select-wrap">
          <select class="sort-select" bind:value={bookmarks.sortBy}>
            <option value="favorites">Sort: Favorites</option>
            <option value="updated">Sort: Updated</option>
            <option value="words">Sort: Word count</option>
          </select>
          <ChevronDown size={13} class="sort-caret" />
        </div>

        <label class="hide-toggle">
          <input type="checkbox" bind:checked={bookmarks.hideRead} /> Hide read
        </label>

        <div class="view-toggle" role="group" aria-label="View">
          <button class:active={viewMode === "list"} onclick={() => (viewMode = "list")} title="List view"><List size={15} /></button>
          <button class:active={viewMode === "card"} onclick={() => (viewMode = "card")} title="Card view"><LayoutGrid size={15} /></button>
        </div>
      </div>
    </div>

    {#if bookmarks.stories.length === 0}
      <p class="empty-note">No bookmarks yet. Bookmark a story from Search to see it here.</p>
    {:else}
      <StoryList
        stories={bookmarks.stories}
        viewMode={viewMode}
        showFandom
        includeDismiss={false}
        onOpenStory={openStory}
        onToggleRead={handleToggleRead}
        onToggleDismissed={handleToggleDismissed}
        onToggleBookmarked={handleToggleBookmarked}
        onDownloadAll={handleDownloadAllChapters}
        downloadingStoryId={bookmarks.downloadingStoryId}
        downloadProgress={bookmarks.downloadDone && bookmarks.downloadTotal ? `Downloading ${bookmarks.downloadDone} of ${bookmarks.downloadTotal}…` : bookmarks.downloadProgress}
      />
    {/if}
  {/if}
</div>
{#if search.modalOpen}
  <SearchModal onClose={() => (search.modalOpen = false)} onImported={(fandom, source) => search.handleImported(fandom, source)} />
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
