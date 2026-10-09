<script lang="ts">
  import { Check, Download, Star, X } from "lucide-svelte";
  import type { StoryRow } from "$lib/types";

  let {
    story,
    includeDismiss,
    onToggleRead,
    onToggleDismissed,
    onToggleBookmarked,
    onDownloadAll,
    downloadingStoryId,
    downloadProgress,
    busy,
    fullyDownloaded,
  }: {
    story: StoryRow;
    includeDismiss: boolean;
    onToggleRead: (s: StoryRow) => void;
    onToggleDismissed: (s: StoryRow) => void;
    onToggleBookmarked: (s: StoryRow) => void;
    onDownloadAll?: (s: StoryRow) => void;
    downloadingStoryId: number | null;
    downloadProgress: string;
    busy: boolean;
    fullyDownloaded: boolean;
  } = $props();
</script>

<div class="row-actions">
  <button
    class="icon-btn"
    class:on-read={story.isRead}
    onclick={() => onToggleRead(story)}
    title="Mark as read"><Check size={13} /></button
  >
  {#if includeDismiss}
    <button
      class="icon-btn"
      class:on-dismiss={story.dismissed}
      onclick={() => onToggleDismissed(story)}
      title="Dismiss — not for me"><X size={13} /></button
    >
  {/if}
  <button
    class="icon-btn"
    class:on-bookmark={story.bookmarked}
    onclick={() => onToggleBookmarked(story)}
    title="Bookmark"
  >
    <Star size={13} fill={story.bookmarked ? "currentColor" : "none"} />
  </button>
  {#if onDownloadAll && (downloadingStoryId === story.id || !fullyDownloaded)}
    <button
      class="icon-btn"
      class:spinning={downloadingStoryId === story.id}
      onclick={() => onDownloadAll(story)}
      disabled={busy}
      title={downloadingStoryId === story.id
        ? downloadProgress
        : busy
          ? "Wait for the current refresh/download to finish"
          : "Download all chapters for offline reading"}
    >
      <Download size={13} />
    </button>
  {/if}
</div>

<style>
  .row-actions {
    display: flex;
    gap: 0.3rem;
    flex-shrink: 0;
  }
  .icon-btn {
    border: 1px solid var(--line);
    background: var(--paper);
    border-radius: 5px;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    color: var(--ink-soft);
  }
  .icon-btn.on-read {
    background: var(--moss-wash);
    color: var(--moss);
    border-color: transparent;
  }
  .icon-btn.on-dismiss {
    background: var(--rust-wash);
    color: var(--rust);
    border-color: transparent;
  }
  .icon-btn.on-bookmark {
    background: var(--gold-wash);
    color: var(--gold);
    border-color: transparent;
  }
  .icon-btn:disabled {
    opacity: 0.6;
  }
  .icon-btn.spinning :global(svg) {
    animation: download-spin 0.9s linear infinite;
  }
  @keyframes download-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
