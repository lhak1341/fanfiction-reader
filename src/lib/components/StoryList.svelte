<script lang="ts">
  // Shared between Search and Bookmarks (locked in wayfinder ticket #5) —
  // list/card rendering only differs by whether fandom shows per-row
  // (Bookmarks spans fandoms; Search is already fandom-scoped) and whether
  // dismiss is offered (not meaningful on an already-bookmarked row).
  import { CloudCheck, Heart, WholeWord } from "lucide-svelte";
  import { fmt } from "$lib/format";
  import RowActions from "$lib/components/RowActions.svelte";
  import StatusBadges from "$lib/components/StatusBadges.svelte";
  import type { StoryRow } from "$lib/types";

  let {
    stories,
    viewMode,
    showFandom = false,
    includeDismiss = true,
    onOpenStory,
    onToggleRead,
    onToggleDismissed,
    onToggleBookmarked,
    onDownloadAll,
    downloadingStoryId = null,
    downloadProgress = "",
    busy = false
  }: {
    stories: StoryRow[];
    viewMode: "list" | "card";
    showFandom?: boolean;
    includeDismiss?: boolean;
    onOpenStory: (s: StoryRow) => void;
    onToggleRead: (s: StoryRow) => void;
    onToggleDismissed: (s: StoryRow) => void;
    onToggleBookmarked: (s: StoryRow) => void;
    // Bookmarks-only "download all chapters" action — omitted on Search.
    onDownloadAll?: (s: StoryRow) => void;
    downloadingStoryId?: number | null;
    downloadProgress?: string;
    // A bulk job (refresh or any download) is running — Download stays disabled.
    busy?: boolean;
  } = $props();

  // AO3 rows can carry a dozen+ freeform tags — capping the chip count
  // (not just each chip's own width) is what actually keeps a card's
  // height in check, since flex-wrap alone just keeps adding rows.
  const MAX_TAG_CHIPS = 4;
  function visibleTags(tags: string): { shown: string[]; extra: number } {
    const all = tags.split(", ");
    return { shown: all.slice(0, MAX_TAG_CHIPS), extra: Math.max(0, all.length - MAX_TAG_CHIPS) };
  }

  function isFullyDownloaded(story: StoryRow): boolean {
    return story.chapterCount > 0 && story.cachedChapterCount >= story.chapterCount;
  }
</script>


{#snippet rowActions(story: StoryRow)}
  <RowActions
    {story}
    {includeDismiss}
    {onToggleRead}
    {onToggleDismissed}
    {onToggleBookmarked}
    {onDownloadAll}
    {downloadingStoryId}
    {downloadProgress}
    {busy}
    fullyDownloaded={isFullyDownloaded(story)}
  />
{/snippet}

{#if viewMode === "list"}
  <div class="list-body">
    {#each stories as story (story.id)}
      <div class="list-row" class:dim={story.isRead || story.dismissed}>
        <div class="main">
          <button class="t link" title={story.title} onclick={() => onOpenStory(story)}>{story.title}</button>
          <div class="meta-line">
            <span>{showFandom ? `${story.author} · ${story.fandom}` : story.author}</span>
            <span class="source-badge" class:ao3={story.source === "ao3"}>{story.source === "ao3" ? "AO3" : "FFN"}</span>
            {#if onDownloadAll && isFullyDownloaded(story)}
              <span class="offline-badge" title="All chapters downloaded — readable offline"><CloudCheck size={11} /> Offline</span>
            {/if}
            <StatusBadges status={story.status} ffnUpdatedAt={story.ffnUpdatedAt} />
          </div>
        </div>
        <div class="desc-col">
          <div class="desc" title={story.summary ?? undefined}>{story.summary ?? ""}</div>
          {#if story.tags}
            <div class="tags-line" title={story.tags}>{story.tags}</div>
          {/if}
        </div>
        <div class="stats">
          <span class="tnum stat-icon" title="Word count"><WholeWord size={12} /> {fmt(story.wordCount)}</span>
          <span class="tnum stat-icon" title="Favorites"><Heart size={12} /> {fmt(story.favoriteCount)}</span>
        </div>
        {@render rowActions(story)}
      </div>
    {/each}
  </div>
{:else}
  <div class="cards">
    {#each stories as story (story.id)}
      <div class="card" class:dim={story.isRead || story.dismissed}>
        <div class="fandom-row">
          <div class="fandom" title={story.fandom}>{story.fandom}</div>
          {#if onDownloadAll && isFullyDownloaded(story)}
            <span class="offline-badge" title="All chapters downloaded — readable offline"><CloudCheck size={11} /> Offline</span>
          {/if}
          <span class="source-badge" class:ao3={story.source === "ao3"}>{story.source === "ao3" ? "AO3" : "FFN"}</span>
        </div>
        <h4><button class="link" onclick={() => onOpenStory(story)}>{story.title}</button></h4>
        <p class="summary" title={story.summary ?? undefined}>{story.summary ?? ""}</p>
        {#if story.tags}
          {@const { shown, extra } = visibleTags(story.tags)}
          <div class="tag-chips">
            {#each shown as t (t)}
              <span class="tag-chip" title={t}>{t}</span>
            {/each}
            {#if extra > 0}
              <span class="tag-chip tag-chip-more" title={story.tags}>+{extra} more</span>
            {/if}
          </div>
        {/if}
        <div class="tags">
          <span class="tag">{story.rating}</span>
          <span class="tag tnum stat-icon" title="Word count"><WholeWord size={11} /> {fmt(story.wordCount)}</span>
          <span class="tag tnum stat-icon" title="Favorites"><Heart size={11} /> {fmt(story.favoriteCount)}</span>
        </div>
        <div class="cardfoot">
          <div class="cardfoot-status">
            <StatusBadges status={story.status} ffnUpdatedAt={story.ffnUpdatedAt} />
          </div>
          {@render rowActions(story)}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .list-body {
    flex: 1;
    overflow-y: auto;
  }
  .list-row {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 0.7rem 1.5rem;
    border-bottom: 1px solid var(--line);
  }
  .list-row.dim {
    opacity: 0.45;
  }
  .list-row .main {
    width: 240px;
    flex-shrink: 0;
    min-width: 0;
  }
  .list-row .main .t {
    display: block;
    width: 100%;
    font-family: var(--serif);
    font-size: 0.95rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .link {
    border: none;
    background: transparent;
    padding: 0;
    margin: 0;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .link:hover {
    text-decoration: underline;
    color: var(--accent);
  }
  .meta-line {
    color: var(--ink-soft);
    font-size: 0.76rem;
    margin-top: 0.15rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    overflow: hidden;
    white-space: nowrap;
  }
  .meta-line span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .desc-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    justify-content: center;
  }
  .desc {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 0.8rem;
    color: var(--ink-soft);
  }
  .tags-line {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 0.72rem;
    color: var(--accent);
  }
  .stats {
    display: flex;
    gap: 0.9rem;
    font-size: 0.78rem;
    color: var(--ink-soft);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .source-badge {
    display: inline-block;
    padding: 0.05rem 0.4rem;
    border-radius: 4px;
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    white-space: nowrap;
    flex-shrink: 0;
    background: var(--paper);
    border: 1px solid var(--line);
    color: var(--ink-soft);
  }
  .source-badge.ao3 {
    background: var(--gold-wash);
    border-color: transparent;
    color: var(--gold);
  }
  .offline-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    padding: 0.05rem 0.4rem;
    border-radius: 4px;
    font-size: 0.62rem;
    font-weight: 700;
    white-space: nowrap;
    flex-shrink: 0;
    background: var(--moss-wash);
    color: var(--moss);
  }
  .stat-icon {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
  }

  .cards {
    flex: 1;
    overflow-y: auto;
    padding: 1.2rem;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 1rem;
    align-content: start;
  }
  .card {
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--paper-raised);
    padding: 1rem;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .card.dim {
    opacity: 0.45;
  }
  .card h4 {
    font-family: var(--serif);
    font-size: 1.02rem;
    font-weight: 600;
  }
  .fandom-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
  }
  .card .fandom {
    font-size: 0.72rem;
    color: var(--accent);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .card p.summary {
    font-size: 0.82rem;
    color: var(--ink-soft);
    line-height: 1.4;
    margin: 0;
    flex: 1;
    display: -webkit-box;
    -webkit-line-clamp: 7;
    line-clamp: 7;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: break-word;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .tag {
    font-size: 0.68rem;
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 0.12rem 0.4rem;
    color: var(--ink-soft);
    max-width: 100px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .tag-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .tag-chip {
    font-size: 0.68rem;
    background: var(--accent-wash);
    border-radius: 999px;
    padding: 0.12rem 0.55rem;
    color: var(--accent);
    max-width: 90px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .tag-chip-more {
    color: var(--ink-soft);
    background: var(--paper);
    border: 1px dashed var(--line);
  }
  .cardfoot {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .cardfoot-status {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
    overflow: hidden;
  }
</style>
