<script lang="ts">
  // Shared between Search (pinnable, full fandom list) and Bookmarks
  // (not pinnable, scoped to fandoms actually present among bookmarks).
  // Locked in wayfinder ticket #5.
  import { ChevronDown, Star } from "lucide-svelte";
  import { fold } from "$lib/normalize";

  let {
    fandoms,
    selected,
    pinnable = false,
    pinnedFandoms = $bindable(new Set<string>()),
    onSelect
  }: {
    fandoms: string[];
    selected: string | undefined;
    pinnable?: boolean;
    pinnedFandoms?: Set<string>;
    onSelect: (f: string | undefined) => void;
  } = $props();

  let open = $state(false);
  let search = $state("");
  let rootEl = $state<HTMLDivElement | undefined>(undefined);

  let visible = $derived(fandoms.filter((f) => fold(f).includes(fold(search))));
  let pinnedVisible = $derived(pinnable ? visible.filter((f) => pinnedFandoms.has(f)) : []);
  let restVisible = $derived(pinnable ? visible.filter((f) => !pinnedFandoms.has(f)) : visible);

  function select(f: string | undefined) {
    onSelect(f);
    open = false;
  }

  function togglePin(f: string, event: MouseEvent) {
    event.stopPropagation();
    const next = new Set(pinnedFandoms);
    next.has(f) ? next.delete(f) : next.add(f);
    pinnedFandoms = next;
  }
</script>

<svelte:window
  onclick={(e) => {
    if (rootEl && !rootEl.contains(e.target as Node)) open = false;
  }}
/>

<div class="fandom-dd" bind:this={rootEl}>
  <button class="fandom-select" onclick={() => (open = !open)}>
    Fandom: {selected ?? "All fandoms"} <ChevronDown size={13} class="caret" />
  </button>
  {#if open}
    <div class="fandom-popover">
      <input class="fandom-search" placeholder="Search fandoms…" bind:value={search} />
      <ul class="fandom-options">
        <li>
          <button class="fandom-opt" class:current={selected === undefined} onclick={() => select(undefined)}>
            All fandoms
          </button>
        </li>
        {#if pinnedVisible.length > 0}
          <li class="fandom-section-label">Pinned</li>
          {#each pinnedVisible as f (f)}
            <li>
              <button class="fandom-pin on" onclick={(e) => togglePin(f, e)} title="Unpin {f}"><Star size={12} fill="currentColor" /></button>
              <button class="fandom-opt" class:current={selected === f} onclick={() => select(f)}>{f}</button>
            </li>
          {/each}
          <li class="fandom-section-label">All fandoms</li>
        {/if}
        {#each restVisible as f (f)}
          <li>
            {#if pinnable}
              <button class="fandom-pin" onclick={(e) => togglePin(f, e)} title="Pin {f}"><Star size={12} /></button>
            {/if}
            <button class="fandom-opt" class:current={selected === f} onclick={() => select(f)}>{f}</button>
          </li>
        {/each}
        {#if visible.length === 0}
          <li class="empty">No fandoms match "{search}"</li>
        {/if}
      </ul>
    </div>
  {/if}
</div>

<style>
  .fandom-dd {
    position: relative;
  }
  .fandom-select {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--accent-wash);
    color: var(--accent);
    padding: 0.42rem 0.8rem;
    font-size: 0.82rem;
    font-weight: 700;
  }
  :global(.caret) {
    opacity: 0.8;
  }
  .fandom-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    width: 260px;
    background: var(--paper-raised);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
    z-index: 40;
    padding: 0.5rem;
  }
  .fandom-search {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font-size: 0.82rem;
    background: var(--paper);
    color: var(--ink);
    margin-bottom: 0.4rem;
  }
  .fandom-options {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 260px;
    overflow-y: auto;
  }
  .fandom-options li {
    display: flex;
    align-items: center;
    gap: 0.15rem;
  }
  .fandom-options button.fandom-opt {
    flex: 1;
    min-width: 0;
    text-align: left;
    border: none;
    background: transparent;
    border-radius: 6px;
    padding: 0.45rem 0.6rem;
    font-size: 0.82rem;
    color: var(--ink);
  }
  .fandom-options button.fandom-opt:hover {
    background: var(--paper);
  }
  .fandom-options button.fandom-opt.current {
    background: var(--accent-wash);
    color: var(--accent);
    font-weight: 700;
  }
  .fandom-options button.fandom-pin {
    flex-shrink: 0;
    border: none;
    background: transparent;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 5px;
    color: var(--ink-soft);
    opacity: 0.35;
  }
  .fandom-options button.fandom-pin:hover {
    opacity: 0.7;
  }
  .fandom-options button.fandom-pin.on {
    color: var(--gold);
    opacity: 1;
  }
  .fandom-options .fandom-section-label {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ink-soft);
    padding: 0.4rem 0.6rem 0.15rem;
  }
  .fandom-options .empty {
    padding: 0.5rem 0.6rem;
    font-size: 0.78rem;
    color: var(--ink-soft);
  }
</style>
