<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as api from "$lib/api";
  import type { StoryRow } from "$lib/types";

  // Fetch spike (SPEC.md §3): confirm the webview-navigation strategy clears
  // Cloudflare on a real story page. Result is printed to the Rust console
  // (the terminal running `bun run tauri dev`), not shown here.
  let spikeUrl = $state("https://www.fanfiction.net/s/8067258/1/");
  let spikeStatus = $state("");

  async function runFetchSpike(event: Event) {
    event.preventDefault();
    spikeStatus = "Opening webview — check the Rust terminal for the result…";
    try {
      await invoke("fetch_page", { url: spikeUrl });
    } catch (e) {
      spikeStatus = `Failed to open spike window: ${e}`;
    }
  }

  // SQLite schema check (SPEC.md §5): confirms every table from the schema
  // exists and reports row counts, straight from the DB.
  let dbStatus = $state("");

  async function checkDb(event: Event) {
    event.preventDefault();
    try {
      const rows = await invoke<string[]>("db_health_check");
      dbStatus = rows.join("\n");
    } catch (e) {
      dbStatus = `DB check failed: ${e}`;
    }
  }

  // Query/toggle commands (SPEC.md §6).
  let storiesStatus = $state("");
  let stories = $state<StoryRow[]>([]);

  async function listStories(event: Event) {
    event.preventDefault();
    try {
      stories = await invoke<StoryRow[]>("list_stories", {
        filters: { sortBy: "favorites", hideReadAndDismissed: false }
      });
      storiesStatus = `${stories.length} stor${stories.length === 1 ? "y" : "ies"}`;
    } catch (e) {
      storiesStatus = `list_stories failed: ${e}`;
    }
  }

  async function listFandoms(event: Event) {
    event.preventDefault();
    try {
      const fandoms = await invoke<string[]>("list_fandoms");
      storiesStatus = `Fandoms: ${fandoms.join(", ") || "(none yet)"}`;
    } catch (e) {
      storiesStatus = `list_fandoms failed: ${e}`;
    }
  }

  async function toggleBookmark(story: StoryRow) {
    try {
      await invoke("toggle_bookmarked", { storyId: story.id, value: !story.bookmarked });
      await listStories(new Event("submit"));
    } catch (e) {
      storiesStatus = `toggle_bookmarked failed: ${e}`;
    }
  }

  // Bulk chapter prefetch — for testing chapter nav (edge zones, keyboard,
  // carousel) without triggering fetches one at a time.
  let prefetchFfnStoryId = $state("8067258");
  let prefetchFrom = $state(2);
  let prefetchTo = $state(5);
  let prefetchStatus = $state("");

  function sleep(ms: number) {
    return new Promise((r) => setTimeout(r, ms));
  }

  async function runPrefetch(event: Event) {
    event.preventDefault();
    try {
      const all = await invoke<StoryRow[]>("list_stories", { filters: {} });
      const target = all.find((s) => s.ffnStoryId === Number(prefetchFfnStoryId));
      if (!target) {
        prefetchStatus = `No story with ffn_story_id ${prefetchFfnStoryId} in the DB yet — fetch chapter 1 first.`;
        return;
      }
      for (let num = prefetchFrom; num <= prefetchTo; num++) {
        prefetchStatus = `Fetching chapter ${num} of ${prefetchTo}…`;
        const url = `https://www.fanfiction.net/s/${prefetchFfnStoryId}/${num}/`;
        try {
          await api.fetchChapterAndWait(url, target.id, num);
        } catch (e) {
          prefetchStatus = `Chapter ${num} failed (${e instanceof Error ? e.message : e}) — re-run with "from: ${num}" to resume.`;
          return;
        }
        // Small human-ish gap between requests — fetching several chapters
        // back-to-back with no pause reads as bot traffic to Cloudflare in a
        // way a single manual fetch doesn't.
        if (num < prefetchTo) await sleep(1500 + Math.random() * 1500);
      }
      prefetchStatus = `Done — chapters ${prefetchFrom}–${prefetchTo} cached.`;
    } catch (e) {
      prefetchStatus = `Prefetch failed: ${e}`;
    }
  }
</script>

<main class="debug">
  <p><a href="/">‹ Back to app</a></p>
  <h1>Debug tools</h1>
  <p>Internal dev tools for exercising the fetch/parse/persist pipeline directly. Not part of the app.</p>

  <hr />
  <h2>Fetch spike</h2>
  <p>Opens a real webview at the URL below, parses the page, and persists it. Result prints to the Rust terminal.</p>
  <form onsubmit={runFetchSpike}>
    <input bind:value={spikeUrl} style="width: 320px" />
    <button type="submit">Run fetch spike</button>
  </form>
  <p>{spikeStatus}</p>

  <hr />
  <h2>Prefetch chapters</h2>
  <p>Fetches a range of chapters sequentially (one webview at a time, waits for each to finish) so chapter nav can be tested without triggering fetches one at a time.</p>
  <form onsubmit={runPrefetch} class="row">
    <label>ffn_story_id <input bind:value={prefetchFfnStoryId} style="width: 100px" /></label>
    <label>from <input type="number" bind:value={prefetchFrom} style="width: 60px" /></label>
    <label>to <input type="number" bind:value={prefetchTo} style="width: 60px" /></label>
    <button type="submit">Prefetch</button>
  </form>
  <p>{prefetchStatus}</p>

  <hr />
  <h2>Database</h2>
  <button onclick={checkDb}>Check DB</button>
  <p style="white-space: pre-line">{dbStatus}</p>

  <hr />
  <h2>Query/toggle commands</h2>
  <div class="row">
    <button onclick={listStories}>List stories</button>
    <button onclick={listFandoms}>List fandoms</button>
  </div>
  <p>{storiesStatus}</p>
  <ul>
    {#each stories as story (story.id)}
      <li>
        <strong>{story.title}</strong> by {story.author} — {story.fandom}, {story.wordCount.toLocaleString()}w,
        ♥{story.favoriteCount.toLocaleString()}, {story.status}
        <button onclick={() => toggleBookmark(story)}>
          {story.bookmarked ? "★ Bookmarked" : "☆ Bookmark"}
        </button>
      </li>
    {/each}
  </ul>
</main>

<style>
  .debug {
    max-width: 640px;
    margin: 2rem auto;
    padding: 0 1.5rem;
    font-family: var(--sans);
  }
  input,
  button {
    font: inherit;
    padding: 0.4rem 0.7rem;
  }
  .row {
    display: flex;
    gap: 0.5rem;
  }
</style>
