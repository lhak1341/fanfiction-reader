// @vitest-environment jsdom
// The library screen (Search and Bookmarks), driven by role against a mocked
// command layer: the same Screens App.tsx builds, with `$lib/api` faked.
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { KitRoot } from "@lhak/style/react";
import type { StoryRow } from "$lib/types";

vi.mock("$lib/api", () => ({
  listStories: vi.fn(),
  listBookmarks: vi.fn(),
  listFandoms: vi.fn(),
  listBookmarkedFandoms: vi.fn(),
  getContinueReading: vi.fn(),
  renameFandom: vi.fn(),
  toggleRead: vi.fn(),
  toggleDismissed: vi.fn(),
  toggleBookmarked: vi.fn(),
  fetchChapterAndWait: vi.fn(),
  ensureChapter: vi.fn(),
  listChapters: vi.fn(),
  getStory: vi.fn(),
  setResumeChapter: vi.fn(),
  listFandomIndex: vi.fn(),
  indexFfnCategoryAndWait: vi.fn(),
  searchFfnAndWait: vi.fn(),
  searchAo3AndWait: vi.fn(),
  addStoryByUrl: vi.fn(),
}));

// The Debug tools screen calls Tauri commands directly (src/CLAUDE.md).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import * as api from "$lib/api";
import { App } from "../App";

function story(over: Partial<StoryRow>): StoryRow {
  return {
    id: 1,
    sourceStoryId: 123,
    title: "A Story",
    author: "Someone",
    fandom: "Naruto",
    rating: "T",
    genre: null,
    language: null,
    tags: null,
    status: "in_progress",
    wordCount: 1000,
    favoriteCount: 10,
    chapterCount: 3,
    summary: null,
    isRead: false,
    dismissed: false,
    bookmarked: false,
    lastChapterNum: null,
    lastScrollPct: null,
    ffnUpdatedAt: null,
    source: "ffn",
    cachedChapterCount: 0,
    ...over,
  };
}

const done = story({ id: 1, title: "Finished Tale", status: "complete" });
const ongoing = story({ id: 2, title: "Ongoing Saga", source: "ao3" });

function renderApp() {
  render(
    <KitRoot>
      <App />
    </KitRoot>
  );
}

/** The row (list item) holding the story with this title. */
const rowOf = (title: string) =>
  screen.getByRole("button", { name: title }).closest("li") as HTMLElement;

const lastCall = <T,>(fn: (arg: T) => unknown) => vi.mocked(fn).mock.calls.at(-1)?.[0];

// jsdom under vitest has no `localStorage` (docs/TESTING.md): a Map stands in.
function stubLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  stubLocalStorage();
  vi.mocked(api.listStories).mockResolvedValue([done, ongoing]);
  vi.mocked(api.listBookmarks).mockResolvedValue([]);
  vi.mocked(api.listFandoms).mockResolvedValue(["Naruto", "Bleach"]);
  vi.mocked(api.listBookmarkedFandoms).mockResolvedValue([]);
  vi.mocked(api.getContinueReading).mockResolvedValue([]);
  vi.mocked(api.toggleRead).mockResolvedValue(undefined);
  vi.mocked(api.toggleBookmarked).mockResolvedValue(undefined);
  vi.mocked(api.renameFandom).mockResolvedValue(1);
  vi.mocked(api.getStory).mockResolvedValue(null);
  vi.mocked(api.listChapters).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Search", () => {
  it("lists the stories with their status as a chip carrying an icon and a word", async () => {
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    const finished = within(rowOf("Finished Tale"));
    expect(finished.getByRole("img", { name: "Success" })).toBeInTheDocument();
    expect(finished.getByText("Done")).toBeInTheDocument();

    const saga = within(rowOf("Ongoing Saga"));
    expect(saga.getByRole("img", { name: "Info" })).toBeInTheDocument();
    expect(saga.getByText("Ongoing")).toBeInTheDocument();
    expect(saga.getByText("AO3")).toBeInTheDocument();
  });

  it("reloads the list with each filter the user changes", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    await user.click(
      within(screen.getByRole("radiogroup", { name: "Source" })).getByRole("radio", {
        name: "AO3",
      })
    );
    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ source: "ao3" }));

    await user.click(
      within(screen.getByRole("radiogroup", { name: "Status" })).getByRole("radio", {
        name: "Done",
      })
    );
    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ status: "complete" }));

    // Hide read starts on (its saved-pref default), so a click turns it off.
    const hideRead = screen.getByRole("checkbox", { name: "Hide read" });
    expect(hideRead).toBeChecked();
    await user.click(hideRead);
    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ hideRead: false }));

    await user.type(screen.getByRole("searchbox", { name: "Search stories" }), "fox");
    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ search: "fox" }));
  });

  it("narrows to a fandom picked from the fandom list", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    await user.click(screen.getByRole("button", { name: /Fandom: All fandoms/ }));
    const list = await screen.findByRole("dialog", { name: "Fandoms" });
    await user.click(within(list).getByRole("button", { name: "Bleach" }));

    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ fandom: "Bleach" }));
    expect(screen.getByRole("button", { name: /Fandom: Bleach/ })).toBeInTheDocument();
  });

  it("pins a fandom and keeps the pin", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    await user.click(screen.getByRole("button", { name: /Fandom: All fandoms/ }));
    const list = await screen.findByRole("dialog", { name: "Fandoms" });
    await user.click(within(list).getByRole("button", { name: "Pin Bleach" }));

    expect(within(list).getByRole("button", { name: "Unpin Bleach" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    await waitFor(() => expect(localStorage.getItem("ffn-pinned-fandoms")).toContain("Bleach"));
  });

  it("marks a story read from its row, then reloads both lists", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });
    const before = vi.mocked(api.listBookmarks).mock.calls.length;

    const actions = screen.getByRole("group", { name: "Actions for Finished Tale" });
    const read = within(actions).getByRole("button", { name: "Mark as read" });
    expect(read).toHaveAttribute("aria-pressed", "false");
    await user.click(read);

    expect(api.toggleRead).toHaveBeenCalledWith(1, true);
    await waitFor(() =>
      expect(vi.mocked(api.listBookmarks).mock.calls.length).toBeGreaterThan(before)
    );
  });

  it("shows a failed load in a banner until the user dismisses it", async () => {
    vi.mocked(api.listStories).mockRejectedValue(new Error("db locked"));
    const user = userEvent.setup();
    renderApp();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Could not load stories: db locked");
    await user.click(within(alert).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("switches to cards and remembers the choice", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    await user.click(screen.getByRole("radio", { name: "Card view" }));

    expect(await screen.findByRole("heading", { name: "Finished Tale" })).toBeInTheDocument();
    expect(localStorage.getItem("ffn-view-mode")).toBe("card");
  });
});

describe("Bookmarks", () => {
  const offline = story({
    id: 3,
    title: "Saved Offline",
    bookmarked: true,
    chapterCount: 2,
    cachedChapterCount: 2,
  });
  const partial = story({ id: 4, title: "Half Saved", bookmarked: true, lastChapterNum: 2 });

  beforeEach(() => {
    vi.mocked(api.listBookmarks).mockResolvedValue([offline, partial]);
    vi.mocked(api.getContinueReading).mockResolvedValue([partial]);
  });

  it("lists bookmarks, with Continue Reading above and offline stories marked", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("tab", { name: "Bookmarks" }));

    const continueReading = await screen.findByRole("region", { name: "Continue Reading" });
    expect(
      within(continueReading).getByRole("button", { name: /Half Saved.*Ch\. 2 of 3/ })
    ).toBeInTheDocument();

    expect(within(rowOf("Saved Offline")).getByText("Offline")).toBeInTheDocument();
    expect(within(rowOf("Half Saved")).queryByText("Offline")).not.toBeInTheDocument();
  });

  it("offers Download only where chapters are missing, and no Dismiss", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("tab", { name: "Bookmarks" }));
    await screen.findByRole("button", { name: "Saved Offline" });

    const full = within(screen.getByRole("group", { name: "Actions for Saved Offline" }));
    const half = within(screen.getByRole("group", { name: "Actions for Half Saved" }));
    expect(
      full.queryByRole("button", { name: "Download all chapters for offline reading" })
    ).not.toBeInTheDocument();
    expect(
      half.getByRole("button", { name: "Download all chapters for offline reading" })
    ).toBeEnabled();
    expect(half.queryByRole("button", { name: /Dismiss/ })).not.toBeInTheDocument();
  });

  it("un-bookmarks a story from its row", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("tab", { name: "Bookmarks" }));
    await screen.findByRole("button", { name: "Half Saved" });

    const actions = screen.getByRole("group", { name: "Actions for Half Saved" });
    await user.click(within(actions).getByRole("button", { name: "Bookmark" }));

    expect(api.toggleBookmarked).toHaveBeenCalledWith(4, false);
  });
});

describe("the Search dialog", () => {
  const openDialog = async (user: ReturnType<typeof userEvent.setup>) => {
    await user.click(screen.getByRole("button", { name: "Search…" }));
    return screen.findByRole("dialog", { name: "Search" });
  };

  it("imports a FanFiction.net fandom picked from the index, then shows it", async () => {
    vi.mocked(api.listFandomIndex).mockResolvedValue([
      { category: "anime", fandom: "Naruto", ffnCount: 450000 },
      { category: "anime", fandom: "Bleach", ffnCount: 80000 },
    ]);
    vi.mocked(api.searchFfnAndWait).mockResolvedValue({ fandom: "Bleach", count: 25 });
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    const dialog = await openDialog(user);
    const submit = within(dialog).getByRole("button", { name: "Search" });
    expect(submit).toBeDisabled();
    const fandom = await within(dialog).findByRole("combobox", { name: "Fandom" });
    await user.type(fandom, "blea");
    const bleach = await screen.findByRole("option", { name: "Bleach" });
    expect(screen.queryByRole("option", { name: "Naruto" })).toBeNull();
    await user.click(bleach);
    expect(fandom).toHaveValue("Bleach");
    await user.click(within(dialog).getByRole("radio", { name: "Done" }));
    await user.click(submit);

    await waitFor(() =>
      expect(api.searchFfnAndWait).toHaveBeenCalledWith("Bleach", 0, "complete", "anime")
    );
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Search" })).toBeNull());
    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ fandom: "Bleach" }));
  });

  it("adds a story by URL and shows Bookmarks, where it now lives", async () => {
    vi.mocked(api.listFandomIndex).mockResolvedValue([]);
    vi.mocked(api.addStoryByUrl).mockResolvedValue({ storyId: 9, chapterNum: 1, chapterCount: 4 });
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    const dialog = await openDialog(user);
    await user.click(within(dialog).getByRole("radio", { name: "By URL" }));
    await user.type(
      within(dialog).getByRole("textbox", { name: "Story URL" }),
      " https://archiveofourown.org/works/1 "
    );
    vi.mocked(api.listBookmarks).mockClear();
    await user.click(within(dialog).getByRole("button", { name: "Add story" }));

    await waitFor(() =>
      expect(api.addStoryByUrl).toHaveBeenCalledWith("https://archiveofourown.org/works/1")
    );
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Search" })).toBeNull());
    expect(screen.getByRole("tab", { name: "Bookmarks" })).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(api.listBookmarks).toHaveBeenCalled());

    // Closed for good, not just unmounted with the Search tab.
    await user.click(screen.getByRole("tab", { name: "Search" }));
    await screen.findByRole("button", { name: "Finished Tale" });
    expect(screen.queryByRole("dialog", { name: "Search" })).toBeNull();
  });

  it("searches AO3 by tag, remembers the tag for the fandom, and shows the fandom", async () => {
    vi.mocked(api.listFandomIndex).mockResolvedValue([]);
    vi.mocked(api.searchAo3AndWait).mockResolvedValue({ fandom: "Naruto", count: 20 });
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    let dialog = await openDialog(user);
    await user.click(within(dialog).getByRole("radio", { name: "Archive of Our Own" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Fandom" }), "Naruto");
    await user.type(within(dialog).getByRole("textbox", { name: "AO3 tag" }), "Naruto (Anime)");
    await user.click(within(dialog).getByRole("radio", { name: "Ongoing" }));
    await user.click(within(dialog).getByRole("radio", { name: "100K+" }));
    await user.click(within(dialog).getByRole("button", { name: "Search" }));

    await waitFor(() =>
      expect(api.searchAo3AndWait).toHaveBeenCalledWith(
        "Naruto (Anime)",
        "Naruto",
        100000,
        "in_progress"
      )
    );
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Search" })).toBeNull());
    await waitFor(() => expect(lastCall(api.listStories)).toMatchObject({ fandom: "Naruto" }));

    dialog = await openDialog(user);
    await user.click(within(dialog).getByRole("radio", { name: "Archive of Our Own" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Fandom" }), "Naruto");
    expect(within(dialog).getByRole("textbox", { name: "AO3 tag" })).toHaveValue("Naruto (Anime)");
  });

  it("keeps the dialog open with the failure shown when a search fails", async () => {
    vi.mocked(api.listFandomIndex).mockResolvedValue([]);
    vi.mocked(api.addStoryByUrl).mockRejectedValue(new Error("Not a story link"));
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    const dialog = await openDialog(user);
    await user.click(within(dialog).getByRole("radio", { name: "By URL" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Story URL" }), "nope");
    await user.click(within(dialog).getByRole("button", { name: "Add story" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Not a story link");
    expect(screen.getByRole("dialog", { name: "Search" })).toBeInTheDocument();
  });

  it("offers to fetch the fandom list while the category has none cached", async () => {
    let indexed = false;
    vi.mocked(api.listFandomIndex).mockImplementation(async () =>
      indexed ? [{ category: "anime", fandom: "Naruto", ffnCount: 5 }] : []
    );
    vi.mocked(api.indexFfnCategoryAndWait).mockImplementation(async () => {
      indexed = true;
      return { category: "anime", count: 1 };
    });
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    const dialog = await openDialog(user);
    await within(dialog).findByRole("button", {
      name: "Index all categories (one-time, ~9 fetches)",
    });
    await user.click(within(dialog).getByRole("combobox", { name: "Category" }));
    await user.click(await screen.findByRole("option", { name: "Anime/Manga" }));
    expect(within(dialog).queryByRole("combobox", { name: "Fandom" })).toBeNull();
    await user.click(
      await within(dialog).findByRole("button", { name: "Fetch fandom list for this category" })
    );

    await waitFor(() => expect(api.indexFfnCategoryAndWait).toHaveBeenCalledWith("anime"));
    expect(await within(dialog).findByRole("combobox", { name: "Fandom" })).toBeInTheDocument();
  });

  it("opens from the empty library", async () => {
    vi.mocked(api.listStories).mockResolvedValue([]);
    vi.mocked(api.listFandomIndex).mockResolvedValue([]);
    const user = userEvent.setup();
    renderApp();

    await screen.findByText("No stories yet.");
    const buttons = screen.getAllByRole("button", { name: "Search…" });
    expect(buttons).toHaveLength(2);
    await user.click(buttons[1]);

    expect(await screen.findByRole("dialog", { name: "Search" })).toBeInTheDocument();
  });
});

describe("the shell", () => {
  it("opens a story at the chapter last read, or chapter 1", async () => {
    const resumed = story({ id: 8, title: "Resumed", lastChapterNum: 3 });
    vi.mocked(api.listStories).mockResolvedValue([resumed, done]);
    vi.mocked(api.getStory).mockImplementation(async (id) => (id === 8 ? resumed : done));
    vi.mocked(api.listChapters).mockResolvedValue([]);
    vi.mocked(api.ensureChapter).mockResolvedValue({
      wasFetched: false,
      chapterCount: null,
      chapters: null,
      content: { title: "A chapter", content: "<p>x</p>", isCached: true },
    });
    vi.mocked(api.setResumeChapter).mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderApp();

    await user.click(await screen.findByRole("button", { name: "Resumed" }));
    await waitFor(() => expect(api.ensureChapter).toHaveBeenCalledWith(8, 3));
    await user.click(screen.getByRole("button", { name: "Back" }));
    await user.click(await screen.findByRole("button", { name: "Finished Tale" }));
    await waitFor(() => expect(api.ensureChapter).toHaveBeenCalledWith(1, 1));
  });

  it("opens a story and comes back to the list it left", async () => {
    vi.mocked(api.listBookmarks).mockResolvedValue([
      story({ id: 5, title: "Kept", bookmarked: true }),
    ]);
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("tab", { name: "Bookmarks" }));
    await user.click(await screen.findByRole("button", { name: "Kept" }));

    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));

    expect(screen.getByRole("tab", { name: "Bookmarks" })).toHaveAttribute("aria-selected", "true");
  });

  it("opens the Debug tools, runs a command there, and comes back", async () => {
    vi.mocked(invoke).mockResolvedValue(["stories: 2", "chapters: 7"]);
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("button", { name: "Finished Tale" });

    await user.click(screen.getByRole("button", { name: "Debug tools" }));
    expect(screen.getByRole("heading", { name: "Debug tools" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Check DB" }));
    expect(invoke).toHaveBeenCalledWith("db_health_check");
    expect(await screen.findByText(/chapters: 7/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("button", { name: "Finished Tale" })).toBeInTheDocument();
  });

  it("keeps the flavor and mode controls in the Appearance popover", async () => {
    const user = userEvent.setup();
    renderApp();

    await user.click(screen.getByRole("button", { name: "Appearance" }));

    const panel = await screen.findByRole("dialog", { name: "Appearance" });
    expect(within(panel).getAllByRole("radiogroup").length).toBeGreaterThanOrEqual(2);
  });
});
