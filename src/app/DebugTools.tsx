// Internal dev tools for exercising the fetch/parse/persist pipeline directly
// (the old /debug route). Commands go through raw `invoke()`, not
// api.ts wrappers: this is a harness for the Tauri commands themselves
// (src/CLAUDE.md). The one `api.fetchChapterAndWait` call, for bulk prefetch,
// is the exception.
import { useState, type FormEvent, type ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Icon, NumberStepper, SettingRow, TextField } from "@lhak/style/react";
import * as api from "$lib/api";
import { jitterDelay, sleep } from "$lib/pacedLoop";
import type { StoryRow } from "$lib/types";

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="fanf-debug-section">
      <h2>{title}</h2>
      {description && <p>{description}</p>}
      {children}
    </section>
  );
}

export function DebugTools({ onBack }: { onBack: () => void }) {
  // Fetch spike (SPEC.md §3): confirm the webview-navigation strategy clears
  // Cloudflare on a real story page. `fetch_page` resolves once the page is
  // fetched and persisted; the Rust console has the step-by-step log.
  const [spikeUrl, setSpikeUrl] = useState("https://www.fanfiction.net/s/8067258/1/");
  const [spikeStatus, setSpikeStatus] = useState("");

  async function runFetchSpike(event: FormEvent) {
    event.preventDefault();
    setSpikeStatus("Opening webview: check the Rust terminal for the result…");
    try {
      const outcome = await invoke("fetch_page", { url: spikeUrl });
      setSpikeStatus(`Fetched and persisted: ${JSON.stringify(outcome)}`);
    } catch (e) {
      setSpikeStatus(`Fetch failed: ${e}`);
    }
  }

  // Bulk chapter prefetch, for testing chapter nav (edge zones, keyboard,
  // carousel) without triggering fetches one at a time.
  const [prefetchFfnStoryId, setPrefetchFfnStoryId] = useState("8067258");
  const [prefetchFrom, setPrefetchFrom] = useState(2);
  const [prefetchTo, setPrefetchTo] = useState(5);
  const [prefetchStatus, setPrefetchStatus] = useState("");

  async function runPrefetch(event: FormEvent) {
    event.preventDefault();
    try {
      // source_story_id is only unique per Source (an AO3 work can share an
      // FFN story's number), so narrow to FFN before matching it.
      const ffnStories = await invoke<StoryRow[]>("list_stories", { filters: { source: "ffn" } });
      const target = ffnStories.find((s) => s.sourceStoryId === Number(prefetchFfnStoryId));
      if (!target) {
        setPrefetchStatus(
          `No FFN story with ffn_story_id ${prefetchFfnStoryId} in the DB yet: fetch chapter 1 first.`
        );
        return;
      }
      for (let num = prefetchFrom; num <= prefetchTo; num++) {
        setPrefetchStatus(`Fetching chapter ${num} of ${prefetchTo}…`);
        try {
          await api.fetchChapterAndWait(target.id, num);
        } catch (e) {
          setPrefetchStatus(
            `Chapter ${num} failed (${e instanceof Error ? e.message : e}): re-run with "from: ${num}" to resume.`
          );
          return;
        }
        // The same human-ish gap every other bulk fetch uses (pacedLoop.ts,
        // architecture review candidate 9, 2026-07-24).
        if (num < prefetchTo) await sleep(jitterDelay());
      }
      setPrefetchStatus(`Done: chapters ${prefetchFrom}–${prefetchTo} cached.`);
    } catch (e) {
      setPrefetchStatus(`Prefetch failed: ${e}`);
    }
  }

  // SQLite schema check (SPEC.md §5): every table from the schema exists,
  // with row counts, straight from the DB.
  const [dbStatus, setDbStatus] = useState("");

  async function checkDb() {
    try {
      const rows = await invoke<string[]>("db_health_check");
      setDbStatus(rows.join("\n"));
    } catch (e) {
      setDbStatus(`DB check failed: ${e}`);
    }
  }

  // Query/toggle commands (SPEC.md §6).
  const [storiesStatus, setStoriesStatus] = useState("");
  const [stories, setStories] = useState<StoryRow[]>([]);

  async function listStories() {
    try {
      const rows = await invoke<StoryRow[]>("list_stories", {
        filters: { sortBy: "favorites", hideRead: false, hideDismissed: false },
      });
      setStories(rows);
      setStoriesStatus(`${rows.length} stor${rows.length === 1 ? "y" : "ies"}`);
    } catch (e) {
      setStoriesStatus(`list_stories failed: ${e}`);
    }
  }

  async function listFandoms() {
    try {
      const fandoms = await invoke<string[]>("list_fandoms");
      setStoriesStatus(`Fandoms: ${fandoms.join(", ") || "(none yet)"}`);
    } catch (e) {
      setStoriesStatus(`list_fandoms failed: ${e}`);
    }
  }

  async function toggleBookmark(story: StoryRow) {
    try {
      await invoke("toggle_bookmarked", { storyId: story.id, value: !story.bookmarked });
      await listStories();
    } catch (e) {
      setStoriesStatus(`toggle_bookmarked failed: ${e}`);
    }
  }

  return (
    <main className="fanf-page fanf-debug">
      <Button variant="ghost" onClick={onBack}>
        <Icon icon={ArrowLeft} />
        Back
      </Button>
      <h1>Debug tools</h1>
      <p>
        Internal dev tools for exercising the fetch/parse/persist pipeline directly. Not part of the
        app.
      </p>

      <Section
        title="Fetch spike"
        description="Opens a real webview at the URL below, parses the page, and persists it. The result prints to the Rust terminal."
      >
        <form className="fanf-debug-row" onSubmit={(e) => void runFetchSpike(e)}>
          <SettingRow label="URL">
            <TextField value={spikeUrl} onChange={(e) => setSpikeUrl(e.currentTarget.value)} />
          </SettingRow>
          <Button type="submit">Run fetch spike</Button>
        </form>
        <p className="fanf-debug-output">{spikeStatus}</p>
      </Section>

      <Section
        title="Prefetch chapters"
        description="Fetches a range of chapters one after another (one webview at a time, waiting for each), so chapter nav can be tested without triggering fetches one at a time."
      >
        <form className="fanf-debug-row" onSubmit={(e) => void runPrefetch(e)}>
          <SettingRow label="ffn_story_id">
            <TextField
              value={prefetchFfnStoryId}
              onChange={(e) => setPrefetchFfnStoryId(e.currentTarget.value)}
            />
          </SettingRow>
          <NumberStepper
            label="From"
            value={prefetchFrom}
            min={1}
            onValueChange={(v) => v !== null && setPrefetchFrom(v)}
          />
          <NumberStepper
            label="To"
            value={prefetchTo}
            min={1}
            onValueChange={(v) => v !== null && setPrefetchTo(v)}
          />
          <Button type="submit">Prefetch</Button>
        </form>
        <p className="fanf-debug-output">{prefetchStatus}</p>
      </Section>

      <Section title="Database">
        <Button onClick={() => void checkDb()}>Check DB</Button>
        <p className="fanf-debug-output">{dbStatus}</p>
      </Section>

      <Section title="Query/toggle commands">
        <div className="fanf-debug-row">
          <Button onClick={() => void listStories()}>List stories</Button>
          <Button onClick={() => void listFandoms()}>List fandoms</Button>
        </div>
        <p className="fanf-debug-output">{storiesStatus}</p>
        <ul className="fanf-debug-list">
          {stories.map((story) => (
            <li key={story.id}>
              <strong>{story.title}</strong> by {story.author}: {story.fandom},{" "}
              {story.wordCount.toLocaleString()}w, ♥{story.favoriteCount.toLocaleString()},{" "}
              {story.status}{" "}
              <Button
                variant="ghost"
                aria-pressed={story.bookmarked}
                onClick={() => void toggleBookmark(story)}
              >
                {story.bookmarked ? "Bookmarked" : "Bookmark"}
              </Button>
            </li>
          ))}
        </ul>
      </Section>
    </main>
  );
}
