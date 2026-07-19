<script lang="ts">
  // Immersive Reader locked in wayfinder ticket #5: progress bar, centered
  // serif prose, no persistent chrome — corner controls fade in on hover.
  // Chapters are fetched lazily (SPEC.md §3/§6): a cached chapter renders
  // instantly, an uncached one triggers a real webview fetch (same
  // Cloudflare/webview strategy as the initial fetch) and waits for the
  // `chapter-fetched` event before rendering.
  import { tick } from "svelte";
  import { ChevronLeft, ChevronRight, Check, Menu, Star, Type, X } from "lucide-svelte";
  import * as api from "$lib/api";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import { getReaderPrefs, saveReaderPrefs, type ReaderPrefs } from "$lib/readerPrefs";
  import type { ChapterContent, ChapterListItem, StoryRow } from "$lib/types";

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

  let story = $state<StoryRow | undefined>(undefined);
  let chapters = $state<ChapterListItem[]>([]);
  let currentChapterNum = $state(initialChapterNum);
  let content = $state<ChapterContent | undefined>(undefined);
  let loading = $state(true);
  let fetchError = $state("");
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
    `font-family: ${fontFamilyCss}; font-size: ${readerPrefs.fontSize}px; font-weight: ${readerPrefs.fontWeight}; line-height: ${readerPrefs.lineHeight}; letter-spacing: ${readerPrefs.letterSpacing}em;`
  );
  // The global h1 reset (app.css) hardcodes serif and zeroes margin — the
  // chapter title needs its own family (following the same prefs as the
  // prose) and its own spacing, not the reset's.
  let titleStyle = $derived(`font-family: ${fontFamilyCss}; letter-spacing: ${readerPrefs.letterSpacing}em;`);

  function handleScroll() {
    if (!scrollEl) return;
    const max = scrollEl.scrollHeight - scrollEl.clientHeight;
    scrollPct = max <= 0 ? 100 : Math.round((scrollEl.scrollTop / max) * 100);
  }

  // Remembers scroll position per chapter for this reading session (not
  // persisted) — so paging back to a chapter you were partway through
  // resumes where you left off, instead of always landing at the top.
  const scrollPositions = new Map<number, number>();

  function sleep(ms: number) {
    return new Promise((r) => setTimeout(r, ms));
  }

  // Bumped on every loadChapter call; a running prefetch chain checks it
  // before each step and bails the moment a real navigation supersedes it.
  let prefetchGen = 0;

  // Quietly warms the two chapters on either side of wherever you land, so
  // paging back and forth feels instant most of the time. Best-effort and
  // low-priority: skips chapters already cached, bails on the generation
  // check if you navigate again mid-chain, and paces itself the same way
  // the debug tool's bulk prefetch does (a fetch burst with no gaps reads
  // as bot traffic to Cloudflare in a way a single manual fetch doesn't).
  async function prefetchNeighbors(gen: number, centerNum: number) {
    if (!story) return;
    const targets = [centerNum - 2, centerNum - 1, centerNum + 1, centerNum + 2].filter(
      (n) => n >= 1 && n <= story!.chapterCount
    );
    for (const n of targets) {
      if (gen !== prefetchGen) return;
      const existing = await api.getChapter(storyId, n);
      if (existing?.isCached) continue;
      if (gen !== prefetchGen) return;
      try {
        const sourceChapterId = chapters.find((c) => c.chapterNum === n)?.sourceChapterId;
        const url = api.chapterFetchUrl(story, n, sourceChapterId);
        await api.fetchChapterAndWait(story.source, url, storyId, n);
        chapters = await api.listChapters(storyId);
      } catch {
        // Best-effort — a failed prefetch just means that chapter fetches
        // normally (foreground) whenever it's actually opened.
      }
      if (gen !== prefetchGen) return;
      await sleep(1500 + Math.random() * 1500);
    }
  }

  async function loadChapter(num: number) {
    const myGen = ++prefetchGen;
    if (scrollEl && content) scrollPositions.set(currentChapterNum, scrollEl.scrollTop);
    loading = true;
    fetchError = "";
    try {
      let chapter = await api.getChapter(storyId, num);
      if (!chapter || !chapter.isCached) {
        if (!story) throw new Error("story metadata not loaded yet");
        const sourceChapterId = chapters.find((c) => c.chapterNum === num)?.sourceChapterId;
        const url = api.chapterFetchUrl(story, num, sourceChapterId);
        await api.fetchChapterAndWait(story.source, url, storyId, num);
        chapters = await api.listChapters(storyId);
        chapter = await api.getChapter(storyId, num);
      }
      content = chapter ?? undefined;
      currentChapterNum = num;
      await tick();
      // rAF, not just tick(): tick() only guarantees Svelte's own DOM
      // updates flushed, not that the browser has finished laying out the
      // newly-inserted chapter HTML — setting scrollTop before that can
      // get silently clamped back to whatever height was measured yet.
      requestAnimationFrame(() => {
        if (!scrollEl) return;
        scrollEl.scrollTop = scrollPositions.get(num) ?? 0;
        scrollEl.focus({ preventScroll: true });
        handleScroll();
      });
      await api.setResumeChapter(storyId, num);
      onChanged?.();
      void prefetchNeighbors(myGen, num);
    } catch (e) {
      fetchError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function init() {
    story = (await api.getStory(storyId)) ?? undefined;
    chapters = await api.listChapters(storyId);
    await loadChapter(initialChapterNum);
  }

  init();

  function goToChapter(num: number) {
    if (num < 1 || num > (story?.chapterCount ?? 1)) return;
    drawerOpen = false;
    loadChapter(num);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "ArrowLeft") goToChapter(currentChapterNum - 1);
    if (e.key === "ArrowRight") goToChapter(currentChapterNum + 1);
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
  let navCenterIdx = $derived(chapters.findIndex((c) => c.chapterNum === currentChapterNum));
  let navWindow = $derived.by(() => {
    if (navCenterIdx === -1) return [];
    return chapters.slice(Math.max(0, navCenterIdx - navRadius), Math.min(chapters.length, navCenterIdx + navRadius + 1));
  });
  // Fades dots out the further they sit from the active chapter — full
  // opacity is restored per-dot on individual hover via CSS (:hover beats
  // this custom property in the cascade, see .nav-row rules below).
  function navDotOpacity(chapterNum: number): number {
    const dist = Math.abs(chapterNum - currentChapterNum);
    return Math.max(0.25, 1 - dist * 0.16);
  }

  async function toggleRead() {
    if (!story) return;
    await api.toggleRead(story.id, !story.isRead);
    story = { ...story, isRead: !story.isRead };
    onChanged?.();
  }

  async function toggleBookmark() {
    if (!story) return;
    await api.toggleBookmarked(story.id, !story.bookmarked);
    story = { ...story, bookmarked: !story.bookmarked };
    onChanged?.();
  }

  async function toggleDismiss() {
    if (!story) return;
    await api.toggleDismissed(story.id, !story.dismissed);
    story = { ...story, dismissed: !story.dismissed };
    onChanged?.();
  }

</script>

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
        class:current={c.chapterNum === currentChapterNum}
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

          <span class="font-label">Size</span>
          <div class="stepper">
            <button onclick={() => updatePrefs({ fontSize: Math.max(14, readerPrefs.fontSize - 1) })} aria-label="Decrease font size">−</button>
            <span class="tnum">{readerPrefs.fontSize}px</span>
            <button onclick={() => updatePrefs({ fontSize: Math.min(28, readerPrefs.fontSize + 1) })} aria-label="Increase font size">+</button>
          </div>

          <span class="font-label">Weight</span>
          <div class="seg">
            <button class:active={readerPrefs.fontWeight === 400} onclick={() => updatePrefs({ fontWeight: 400 })}>Light</button>
            <button class:active={readerPrefs.fontWeight === 500} onclick={() => updatePrefs({ fontWeight: 500 })}>Regular</button>
            <button class:active={readerPrefs.fontWeight === 700} onclick={() => updatePrefs({ fontWeight: 700 })}>Bold</button>
          </div>

          <span class="font-label">Line height</span>
          <div class="stepper">
            <button onclick={() => updatePrefs({ lineHeight: Math.round((Math.max(1.2, readerPrefs.lineHeight - 0.1)) * 100) / 100 })} aria-label="Decrease line height">−</button>
            <span class="tnum">{readerPrefs.lineHeight.toFixed(2)}</span>
            <button onclick={() => updatePrefs({ lineHeight: Math.round((Math.min(2.4, readerPrefs.lineHeight + 0.1)) * 100) / 100 })} aria-label="Increase line height">+</button>
          </div>

          <span class="font-label">Kerning</span>
          <div class="stepper">
            <button onclick={() => updatePrefs({ letterSpacing: Math.round((Math.max(-0.05, readerPrefs.letterSpacing - 0.01)) * 1000) / 1000 })} aria-label="Decrease letter spacing">−</button>
            <span class="tnum">{readerPrefs.letterSpacing.toFixed(2)}em</span>
            <button onclick={() => updatePrefs({ letterSpacing: Math.round((Math.min(0.2, readerPrefs.letterSpacing + 0.01)) * 1000) / 1000 })} aria-label="Increase letter spacing">+</button>
          </div>

          <span class="font-label">Page width</span>
          <div class="stepper">
            <button onclick={() => updatePrefs({ pageWidth: Math.max(40, readerPrefs.pageWidth - 2) })} aria-label="Decrease page width">−</button>
            <span class="tnum">{readerPrefs.pageWidth}ch</span>
            <button onclick={() => updatePrefs({ pageWidth: Math.min(90, readerPrefs.pageWidth + 2) })} aria-label="Increase page width">+</button>
          </div>
        </div>
      {/if}
    </div>
  </div>
  <div class="corner-cluster right">
    <ThemeToggle />
  </div>

  <button class="edge-zone left" aria-label="Previous chapter" disabled={currentChapterNum <= 1} onclick={() => goToChapter(currentChapterNum - 1)}><ChevronLeft size={22} /></button>
  <button class="edge-zone right" aria-label="Next chapter" disabled={!story || currentChapterNum >= story.chapterCount} onclick={() => goToChapter(currentChapterNum + 1)}><ChevronRight size={22} /></button>

  <div class="readerC-scroll" bind:this={scrollEl} onscroll={handleScroll} tabindex="-1">
    <div class="readerC-inner" style="max-width: {readerPrefs.pageWidth}ch">
      {#if story}
        <span class="readerC-chip">Chapter {currentChapterNum} of {story.chapterCount}</span>
      {/if}
      {#if loading}
        <p class="status">Loading…</p>
      {:else if fetchError}
        <p class="status error">{fetchError}</p>
      {:else if content}
        <h1 class="chapter-title" style={titleStyle}>{content.title}</h1>
        <div class="prose" style={proseStyle}>
          {@html content.content}
        </div>
      {/if}
    </div>
  </div>

  {#if story}
    <div class="fab-cluster">
      <button class="fab" class:bookmarked={story.bookmarked} onclick={toggleBookmark} aria-pressed={story.bookmarked} title={story.bookmarked ? "Bookmarked" : "Bookmark"}>
        <Star size={17} fill={story.bookmarked ? "currentColor" : "none"} />
      </button>
      <button class="fab" class:dismissed={story.dismissed} onclick={toggleDismiss} aria-pressed={story.dismissed} title={story.dismissed ? "Dismissed" : "Dismiss"}>
        <X size={17} />
      </button>
      <button class="fab fab-primary" class:on={story.isRead} onclick={toggleRead} aria-pressed={story.isRead} title="Mark as read"><Check size={20} /></button>
    </div>
  {/if}
</div>

{#if drawerOpen}
  <div class="drawer-overlay" onclick={() => (drawerOpen = false)} onkeydown={(e) => e.key === "Escape" && (drawerOpen = false)} role="button" tabindex="-1"></div>
  <aside class="chapter-drawer">
    <div class="storyhead">
      <div>
        <h2>{story?.title ?? ""}</h2>
        <div class="author">by {story?.author ?? ""}</div>
      </div>
      <button class="close" onclick={() => (drawerOpen = false)} aria-label="Close"><X size={15} /></button>
    </div>
    <ul class="chapter-ul">
      {#each chapters as c (c.chapterNum)}
        <li>
          <button aria-current={c.chapterNum === currentChapterNum} onclick={() => goToChapter(c.chapterNum)}>
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
  .seg,
  .stepper {
    display: flex;
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    flex: 1;
  }
  .seg button,
  .stepper button {
    background: var(--paper);
    border: none;
    color: var(--ink-soft);
    padding: 0.3rem 0.5rem;
    font-size: 0.72rem;
    border-right: 1px solid var(--line);
  }
  .seg button:last-child,
  .stepper button:last-child {
    border-right: none;
  }
  .seg button.active {
    background: var(--accent-wash);
    color: var(--accent);
    font-weight: 700;
  }
  .stepper {
    align-items: center;
  }
  .stepper button {
    flex-shrink: 0;
  }
  .stepper span {
    flex: 1;
    text-align: center;
    padding: 0 0.4rem;
    font-size: 0.76rem;
    color: var(--ink);
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
