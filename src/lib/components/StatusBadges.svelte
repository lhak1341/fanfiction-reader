<script lang="ts">
  import { Calendar } from "lucide-svelte";
  import { shortDate } from "$lib/format";
  import type { StoryStatus } from "$lib/types";

  let {
    status,
    ffnUpdatedAt,
  }: {
    status: StoryStatus;
    ffnUpdatedAt: string | null;
  } = $props();
</script>

<span class="pill" class:complete={status === "complete"}>
  {status === "complete" ? "Done" : "Ongoing"}
</span>
{#if status === "in_progress" && ffnUpdatedAt && shortDate(ffnUpdatedAt)}
  <span class="updated-note" title="Last updated {ffnUpdatedAt}"
    ><Calendar size={11} /> {shortDate(ffnUpdatedAt)}</span
  >
{/if}

<style>
  .pill {
    display: inline-block;
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
    font-size: 0.65rem;
    font-weight: 700;
    white-space: nowrap;
    flex-shrink: 0;
    background: var(--accent-wash);
    color: var(--accent);
  }
  .pill.complete {
    background: var(--moss-wash);
    color: var(--moss);
  }
  .updated-note {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.7rem;
    color: var(--ink-soft);
    white-space: nowrap;
    flex-shrink: 0;
  }
</style>
