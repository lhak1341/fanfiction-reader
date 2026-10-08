<script lang="ts">
  // Immersive Reader locked in wayfinder ticket #5: progress bar, centered
  // serif prose, no persistent chrome — corner controls fade in on hover.
  // Chapters are fetched lazily (SPEC.md §3/§6): a cached chapter renders
  // instantly, an uncached one triggers a real webview fetch (same
  // Cloudflare/webview strategy as the initial fetch) and waits for the
  // fetch to resolve before rendering.
  import { onDestroy, tick } from "svelte";
  import { ChevronLeft, ChevronRight, Check, Menu, Star, Type, X } from "lucide-svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import Stepper from "$lib/components/Stepper.svelte";
  import { getReaderPrefs, saveReaderPrefs, type ReaderPrefs } from "$lib/readerPrefs";
  import { smartenQuotesHtml } from "$lib/typography";
  import { ReaderScreen } from "$lib/readerScreen.svelte";

  let {
    storyId,
    initialChapterNum,
    onBack,
    onChanged
  }: {
    storyId: number;
    initialChapterNum: number;
    onBack: () => void;
    onChanged?: () => void;
  } = $props();

  // Captured once, deliberately: Reader always fully remounts per story
  // (+page.svelte's `{#if screen === "reader"}` toggles from a list screen),
  // so storyId/onChanged never change during one mount's lifetime.
  // svelte-ignore state_referenced_locally
  const screen = new ReaderScreen(storyId, onChanged);
  onDestroy(() => screen.dispose());

  let drawerOpen = $state(false);
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let scrollPct = $state(0);

  let readerPrefs = $state<ReaderPrefs>(getReaderPrefs());
  let fontPanelOpen = $state(false);
  let fontPanelEl = $state<HTMLDivElement | undefined>(undefined);

  function updatePrefs(patch: Partial<ReaderPrefs>) {
    readerPrefs = { ...readerPrefs, ...patch };
    saveReaderPrefs(readerPrefs);
  }

  // Custom font takes priority when set; falls back to the serif/sans
  // fallback stack if it isn't installed locally (nothing is downloaded/bundled).
  let fontFamilyCss = $derived.by(() => {
    const fallback = `var(${readerPrefs.fontFamily === "sans" ? "--sans" : "--serif"})`;
    return readerPrefs.customFont.trim() ? `"${readerPrefs.customFont.trim()}", ${fallback}` : fallback;
  });
  let proseStyle = $derived(
    `font-family: ${fontFamilyCss}; font-size: ${readerPrefs.fontSize}px; font-weight: ${readerPrefs.fontWeight}; line-height: ${readerPrefs.lineHeight}; letter-spacing: ${readerPrefs.letterSpacing}em; --bold-weight: ${readerPrefs.fontWeightBold};`
  );
  // The global h1 reset (app.css) hardcodes serif and zeroes margin — the
  // chapter title needs its own family (following the same prefs as the
  // prose) and its own spacing, not the reset's.
  let titleStyle = $derived(`font-family: ${fontFamilyCss}; letter-spacing: ${readerPrefs.letterSpacing}em;`);

  let displayedHtml = $derived.by(() => {
    if (!screen.content) return "";
    return readerPrefs.smartQuotes ? smartenQuotesHtml(screen.content.content) : screen.content.content;
  });

  function handleScroll() {
    if (!scrollEl) return;
    const max = scrollEl.scrollHeight - scrollEl.clientHeight;
    scrollPct = max <= 0 ? 100 : Math.round((scrollEl.scrollTop / max) * 100);
    screen.reportScroll(scrollPct);
  }

  // ReaderScreen owns the generation-token/prefetch/scroll-position-map
  // logic; this component only ever hands it a plain scrollTop number and,
  // once it resolves, applies the restore to the real DOM node.
  async function restoreScrollAndFocus(num: number) {
    await tick();
    // rAF, not just tick(): tick() only guarantees Svelte's own DOM
    // updates flushed, not that the browser has finished laying out the
    // newly-inserted chapter HTML — setting scrollTop before that can
    // get silently clamped back to whatever height was measured yet.
    requestAnimationFrame(() => {
      if (!scrollEl) return;
      // This session's exact pixel position if there is one, else the saved
      // percentage for the chapter you last read (0 — the top — for any other).
      const max = scrollEl.scrollHeight - scrollEl.clientHeight;
      scrollEl.scrollTop = screen.scrollPositions.get(num) ?? (max > 0 ? (screen.resumePctFor(num) / 100) * max : 0);
      scrollEl.focus({ preventScroll: true });
      handleScroll();
    });
  }

  // svelte-ignore state_referenced_locally
  screen.init(initialChapterNum).then(() => restoreScrollAndFocus(screen.currentChapterNum));

  async function goToChapter(num: number) {
    drawerOpen = false;
    await screen.goToChapter(num, scrollEl?.scrollTop);
    await restoreScrollAndFocus(screen.currentChapterNum);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "ArrowLeft") goToChapter(screen.currentChapterNum - 1);
    if (e.key === "ArrowRight") goToChapter(screen.currentChapterNum + 1);
  }

  // Carousel window for the hover-reveal chapter strip: current chapter
  // plus as many before/after as fit the window's height, not the full TOC
  // (that's what the drawer is for). The strip is vertically centered
  // (`top: 50%`), so NAV_VERTICAL_MARGIN reserves room top and bottom to
  // keep it clear of the corner buttons and the fab cluster.
  const NAV_ROW_HEIGHT = 20;
  const NAV_VERTICAL_MARGIN = 220;
  let viewportHeight = $state(0);
  let navRadius = $derived.by(() => {
    const maxRows = Math.max(1, Math.floor((viewportHeight - NAV_VERTICAL_MARGIN) / NAV_ROW_HEIGHT));
    return Math.max(2, Math.floor((maxRows - 1) / 2));
  });
  let navHover = $state(false);
  let navCenterIdx = $derived(screen.chapters.findIndex((c) => c.chapterNum === screen.currentChapterNum));
  let navWindow = $derived.by(() => {
    if (navCenterIdx === -1) return [];
    return screen.chapters.slice(Math.max(0, navCenterIdx - navRadius), Math.min(screen.chapters.length, navCenterIdx + navRadius + 1));
  });
  // Fades dots out the further they sit from the active chapter — full
  // opacity is restored per-dot on individual hover via CSS (:hover beats
  // this custom property in the cascade, see .nav-row rules below).
  function navDotOpacity(chapterNum: number): number {
    const dist = Math.abs(chapterNum - screen.currentChapterNum);
    return Math.max(0.25, 1 - dist * 0.16);
  }
</script>

<!-- Save the reading position when the window is hidden: quitting from the Reader leaves no other hook. -->
<svelte:document onvisibilitychange={() => document.hidden && screen.flush()} />

<svelte:window
  bind:innerHeight={viewportHeight}
  onkeydown={handleKeydown}
  onclick={(e) => {
    if (fontPanelEl && !fontPanelEl.contains(e.target as Node)) fontPanelOpen = false;
  }}
/>

<div class="readerC">
  <div class="progress-track"><div class="progress-fill" style="width: {scrollPct}%"></div></div>

  <div
    class="chapter-nav"
    role="group"
    aria-label="Nearby chapters"
    class:hover={navHover}
    onmouseenter={() => (navHover = true)}
    onmouseleave={() => (navHover = false)}
  >
    {#each navWindow as c (c.chapterNum)}
      <button
        class="nav-row"
        class:current={c.chapterNum === screen.currentChapterNum}
        style="--dist-opacity: {navDotOpacity(c.chapterNum)}"
        onclick={() => goToChapter(c.chapterNum)}
      >
        <span class="nav-dot"></span>
        <span class="nav-label">{c.chapterNum}. {c.title}</span>
      </button>
    {/each}
  </div>

  <div class="corner-cluster left">
    <button class="corner-btn" onclick={onBack}><ChevronLeft size={13} /> Back</button>
    <button class="corner-btn icon-only" onclick={() => (drawerOpen = true)} title="Chapter list"><Menu size={14} /></button>
    <div class="font-dd" bind:this={fontPanelEl}>
      <button class="corner-btn icon-only" onclick={() => (fontPanelOpen = !fontPanelOpen)} title="Reading font">
        <Type size={14} />
      </button>
      {#if fontPanelOpen}
        <div class="font-panel">
          <span class="font-label">Font</span>
          <div class="seg">
            <button class:active={readerPrefs.fontFamily === "serif"} onclick={() => updatePrefs({ fontFamily: "serif" })}>Serif</button>
            <button class:active={readerPrefs.fontFamily === "sans"} onclick={() => updatePrefs({ fontFamily: "sans" })}>Sans</button>
          </div>

          <span class="font-label">Custom font</span>
          <input
            class="font-input"
            placeholder="e.g. Bookerly"
            value={readerPrefs.customFont}
            onchange={(e) => updatePrefs({ customFont: e.currentTarget.value })}
          />

          <Stepper
            label="Size"
            value={readerPrefs.fontSize}
            min={14}
            max={28}
            step={1}
            unit="px"
            onchange={(v) => updatePrefs({ fontSize: v })}
          />

          <Stepper
            label="Weight"
            value={readerPrefs.fontWeight}
            min={100}
            max={900}
            step={100}
            onchange={(v) => updatePrefs({ fontWeight: v })}
          />

          <Stepper
            label="Bold weight"
            value={readerPrefs.fontWeightBold}
            min={100}
            max={900}
            step={100}
            onchange={(v) => updatePrefs({ fontWeightBold: v })}
          />

          <Stepper
            label="Line height"
            value={readerPrefs.lineHeight}
            min={1.2}
            max={2.4}
            step={0.1}
            decimals={2}
            onchange={(v) => updatePrefs({ lineHeight: v })}
          />

          <Stepper
            label="Kerning"
            value={readerPrefs.letterSpacing}
            min={-0.05}
            max={0.2}
            step={0.01}
            decimals={3}
            unit="em"
            onchange={(v) => updatePrefs({ letterSpacing: v })}
          />

          <Stepper
            label="Page width"
            value={readerPrefs.pageWidth}
            min={40}
            max={90}
            step={2}
            unit="ch"
            onchange={(v) => updatePrefs({ pageWidth: v })}
          />

          <span class="font-label">Smart quotes</span>
          <div class="seg">
            <button class:active={readerPrefs.smartQuotes} onclick={() => updatePrefs({ smartQuotes: true })}>On</button>
            <button class:active={!readerPrefs.smartQuotes} onclick={() => updatePrefs({ smartQuotes: false })}>Off</button>
          </div>
        </div>
      {/if}
    </div>
  </div>
  <div class="corner-cluster right">
    <ThemeToggle />
  </div>

  <button class="edge-zone left" aria-label="Previous chapter" disabled={screen.currentChapterNum <= 1} onclick={() => goToChapter(screen.currentChapterNum - 1)}><ChevronLeft size={22} /></button>
  <button class="edge-zone right" aria-label="Next chapter" disabled={!screen.story || screen.currentChapterNum >= screen.story.chapterCount} onclick={() => goToChapter(screen.currentChapterNum + 1)}><ChevronRight size={22} /></button>

  <div class="readerC-scroll" bind:this={scrollEl} onscroll={handleScroll} tabindex="-1">
    <div class="readerC-inner" style="max-width: {readerPrefs.pageWidth}ch">
      {#if screen.story}
        <span class="readerC-chip">Chapter {screen.currentChapterNum} of {screen.story.chapterCount}</span>
      {/if}
      {#if screen.loading}
        <p class="status">Loading…</p>
      {:else if screen.fetchError}
        <p class="status error">{screen.fetchError}</p>
      {:else if screen.content}
        <h1 class="chapter-title" style={titleStyle}>{screen.content.title}</h1>
        <div class="prose" style={proseStyle}>
          {@html displayedHtml}
        </div>
      {/if}
    </div>
  </div>

  {#if screen.story}
    <div class="fab-cluster">
      {#if screen.toggleError}
        <p class="status error fab-error">{screen.toggleError}</p>
      {/if}
      <button class="fab" class:bookmarked={screen.story.bookmarked} onclick={() => screen.toggleBookmark()} aria-pressed={screen.story.bookmarked} title={screen.story.bookmarked ? "Bookmarked" : "Bookmark"}>
        <Star size={17} fill={screen.story.bookmarked ? "currentColor" : "none"} />
      </button>
      <button class="fab" class:dismissed={screen.story.dismissed} onclick={() => screen.toggleDismiss()} aria-pressed={screen.story.dismissed} title={screen.story.dismissed ? "Dismissed" : "Dismiss"}>
        <X size={17} />
      </button>
      <button class="fab fab-primary" class:on={screen.story.isRead} onclick={() => screen.toggleRead()} aria-pressed={screen.story.isRead} title="Mark as read"><Check size={20} /></button>
    </div>
  {/if}
</div>

{#if drawerOpen}
  <div class="drawer-overlay" onclick={() => (drawerOpen = false)} onkeydown={(e) => e.key === "Escape" && (drawerOpen = false)} role="button" tabindex="-1"></div>
  <aside class="chapter-drawer">
    <div class="storyhead">
      <div>
        <h2>{screen.story?.title ?? ""}</h2>
        <div class="author">by {screen.story?.author ?? ""}</div>
      </div>
      <button class="close" onclick={() => (drawerOpen = false)} aria-label="Close"><X size={15} /></button>
    </div>
    <ul class="chapter-ul">
      {#each screen.chapters as c (c.chapterNum)}
        <li>
          <button aria-current={c.chapterNum === screen.currentChapterNum} onclick={() => goToChapter(c.chapterNum)}>
            <span class="n tnum">{c.chapterNum}</span>
            <span>{c.title}</span>
            {#if !c.isCached}<span class="uncached" title="Not fetched yet">○</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  </aside>
{/if}

<style>
  .readerC {
    height: 100vh;
    position: relative;
    background: var(--paper);
  }
  .progress-track {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 3px;
    background: var(--line);
    z-index: 5;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
  }

  /* Layered above .edge-zone.left (paints later in DOM order, same fixed
     stacking context) so clicks on a dot/label hit this, not the
     prev-chapter click zone underneath. */
  .chapter-nav {
    position: fixed;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    flex-direction: column;
    gap: 3px;
    z-index: 22;
    opacity: 0.3;
    transition: opacity 0.15s;
  }
  .chapter-nav.hover,
  .chapter-nav:focus-within {
    opacity: 1;
  }
  .nav-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    border: none;
    background: transparent;
    padding: 3px 0;
    opacity: var(--dist-opacity, 1);
    transition: opacity 0.15s;
  }
  .nav-row:hover,
  .nav-row.current {
    opacity: 1;
  }
  .nav-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ink-soft);
    flex-shrink: 0;
  }
  .nav-row.current .nav-dot {
    width: 8px;
    height: 8px;
    background: var(--accent);
  }
  .nav-label {
    font-size: 0.7rem;
    color: var(--ink-soft);
    white-space: nowrap;
    max-width: 0;
    overflow: hidden;
    transition: max-width 0.15s;
  }
  .chapter-nav.hover .nav-label,
  .chapter-nav:focus-within .nav-label {
    max-width: 220px;
  }
  .nav-row.current .nav-label {
    color: var(--accent);
    font-weight: 700;
  }

  .readerC-scroll {
    height: 100%;
    outline: none;
    overflow-y: auto;
  }
  .readerC-inner {
    max-width: 58ch;
    margin: 0 auto;
    padding: 4.5rem 1.5rem 10rem;
  }
  .readerC-chip {
    display: inline-block;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--accent);
    background: var(--accent-wash);
    border-radius: 999px;
    padding: 0.2rem 0.65rem;
    margin-bottom: 1rem;
  }
  .status {
    color: var(--ink-soft);
    font-family: var(--sans);
  }
  .status.error {
    color: var(--rust);
  }
  .chapter-title {
    margin-bottom: 1.75rem;
  }
  .prose {
    font-family: var(--serif);
    font-size: 1.08rem;
    line-height: 1.75;
    color: var(--ink);
  }
  .prose :global(p) {
    margin: 0 0 1.15em;
  }
  .prose :global(b),
  .prose :global(strong) {
    font-weight: var(--bold-weight, 700);
  }
  .prose :global(hr) {
    border: none;
    border-top: 1px solid var(--line);
    margin: 1.5em 0;
  }

  .edge-zone {
    position: fixed;
    top: 3px;
    bottom: 0;
    width: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ink-soft);
    opacity: 0;
    transition: opacity 0.15s;
    border: none;
    background: transparent;
    font-size: 1.2rem;
  }
  .edge-zone:hover:not(:disabled),
  .edge-zone:focus-visible:not(:disabled) {
    opacity: 1;
  }
  .edge-zone.left {
    left: 0;
  }
  .edge-zone.right {
    right: 0;
  }
  .edge-zone:disabled {
    cursor: default;
  }

  .fab-cluster {
    position: fixed;
    right: 1.5rem;
    bottom: 1.5rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.65rem;
    z-index: 10;
  }
  .fab-error {
    max-width: 160px;
    text-align: right;
    font-size: 0.7rem;
    background: var(--paper-raised);
    border-radius: 8px;
    padding: 0.35rem 0.55rem;
    box-shadow: var(--shadow);
  }
  .fab {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--paper-raised);
    color: var(--ink-soft);
    border: 1px solid var(--line);
    box-shadow: var(--shadow);
    display: grid;
    place-items: center;
    opacity: 0.3;
    transition: opacity 0.15s;
  }
  .fab:hover,
  .fab:focus-visible {
    opacity: 1;
  }
  .fab-primary {
    width: 52px;
    height: 52px;
    background: var(--ink);
    color: var(--paper);
    border: none;
  }
  .fab-primary.on {
    background: var(--moss);
    color: var(--paper-raised);
  }
  .fab.bookmarked {
    background: var(--gold-wash);
    color: var(--gold);
    border-color: transparent;
  }
  .fab.dismissed {
    background: var(--rust-wash);
    color: var(--rust);
    border-color: transparent;
  }

  .corner-cluster {
    position: fixed;
    top: 14px;
    display: flex;
    gap: 0.4rem;
    z-index: 20;
    opacity: 0.4;
    transition: opacity 0.15s;
  }
  .corner-cluster:hover,
  .corner-cluster:focus-within {
    opacity: 1;
  }
  .corner-cluster.left {
    left: 1.5rem;
  }
  .corner-cluster.right {
    right: 1.5rem;
  }
  .corner-btn {
    border: 1px solid var(--line);
    background: var(--paper-raised);
    color: var(--ink-soft);
    border-radius: 999px;
    height: 30px;
    padding: 0 0.7rem;
    font-size: 0.76rem;
    box-shadow: var(--shadow);
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .corner-btn.icon-only {
    width: 30px;
    padding: 0;
    display: grid;
    place-items: center;
  }

  .font-dd {
    position: relative;
  }
  .font-panel {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    width: 260px;
    background: var(--paper-raised);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
    z-index: 25;
    padding: 0.7rem 0.8rem;
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    row-gap: 0.55rem;
    column-gap: 0.7rem;
    font-family: var(--sans);
  }
  .font-label {
    font-size: 0.72rem;
    color: var(--ink-soft);
    white-space: nowrap;
  }
  .font-input {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.25rem 0.45rem;
    font-size: 0.76rem;
    background: var(--paper);
    color: var(--ink);
  }
  .seg {
    display: flex;
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    background: var(--paper);
    border: none;
    color: var(--ink-soft);
    padding: 0.3rem 0.5rem;
    font-size: 0.72rem;
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

  .drawer-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    z-index: 29;
  }
  .chapter-drawer {
    position: fixed;
    top: 0;
    bottom: 0;
    left: 0;
    width: 260px;
    background: var(--paper-raised);
    border-right: 1px solid var(--line);
    z-index: 30;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow);
  }
  .storyhead {
    padding: 1.1rem 1.1rem 0.8rem;
    border-bottom: 1px solid var(--line);
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .storyhead h2 {
    font-size: 0.95rem;
  }
  .storyhead .author {
    font-size: 0.74rem;
    color: var(--ink-soft);
    margin-top: 0.2rem;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    font-size: 1rem;
    line-height: 1;
    padding: 0.2rem;
  }
  .chapter-ul {
    list-style: none;
    margin: 0;
    padding: 0.5rem;
    overflow-y: auto;
    flex: 1;
  }
  .chapter-ul li button {
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    border-radius: 6px;
    padding: 0.5rem 0.7rem;
    font-size: 0.8rem;
    color: var(--ink-soft);
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .chapter-ul li button .n {
    color: var(--ink-soft);
    width: 1.4rem;
  }
  .chapter-ul li button .uncached {
    margin-left: auto;
    font-size: 0.6rem;
  }
  .chapter-ul li button[aria-current="true"] {
    background: var(--accent-wash);
    color: var(--accent);
    font-weight: 700;
  }
  .chapter-ul li button[aria-current="true"] .n {
    color: var(--accent);
  }
</style>
