// @vitest-environment jsdom
// The reading view, driven by role against a mocked command layer: the same
// ReaderScreen the app builds, with `$lib/api` faked. jsdom has no layout, so
// scroll extents are set by hand where a test needs them.
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { KitRoot } from "@lhak/style/react";
import type { ChapterContent, StoryRow } from "$lib/types";

vi.mock("$lib/api", () => ({
  getStory: vi.fn(),
  listChapters: vi.fn(),
  ensureChapter: vi.fn(),
  setResumeChapter: vi.fn(),
  toggleRead: vi.fn(),
  toggleDismissed: vi.fn(),
  toggleBookmarked: vi.fn(),
}));

import * as api from "$lib/api";
import { Reader } from "../Reader";

const STORY: StoryRow = {
  id: 7,
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
  lastChapterNum: 2,
  lastScrollPct: 40,
  ffnUpdatedAt: null,
  source: "ffn",
  cachedChapterCount: 3,
};

const CHAPTERS = [1, 2, 3].map((n) => ({ chapterNum: n, title: `Title ${n}`, isCached: true }));

const chapter = (n: number): ChapterContent => ({
  title: `Chapter ${n}: Title ${n}`,
  content: `<p>Body of chapter ${n}, "quoted".</p><p style="text-align:center;">* * *</p>`,
  isCached: true,
});

function stubLocalStorage(saved?: object) {
  const store = new Map<string, string>();
  if (saved) store.set("ffn-reader-typography", JSON.stringify(saved));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
  return store;
}

const onBack = vi.fn();

function renderReader(initialChapterNum = 2) {
  render(
    <KitRoot>
      <Reader storyId={STORY.id} initialChapterNum={initialChapterNum} onBack={onBack} />
    </KitRoot>
  );
}

const prose = () => document.querySelector(".fanf-prose") as HTMLElement;
const savedPrefs = (store: Map<string, string>) =>
  JSON.parse(store.get("ffn-reader-typography") ?? "{}");

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getStory).mockResolvedValue(STORY);
  vi.mocked(api.listChapters).mockResolvedValue(CHAPTERS);
  vi.mocked(api.ensureChapter).mockImplementation(async (_id, n) => ({
    wasFetched: false,
    content: chapter(n),
    chapterCount: null,
    chapters: null,
  }));
  vi.mocked(api.setResumeChapter).mockResolvedValue(undefined);
  vi.mocked(api.toggleBookmarked).mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("Reader", () => {
  it("opens on the chapter it was given, title as the page heading", async () => {
    stubLocalStorage();
    renderReader();

    expect(
      await screen.findByRole("heading", { level: 1, name: "Chapter 2: Title 2" })
    ).toBeInTheDocument();
    expect(screen.getByText("Chapter 2 of 3")).toBeInTheDocument();
    expect(screen.getByText("Body of chapter 2, “quoted”.")).toBeInTheDocument();
    expect(api.ensureChapter).toHaveBeenCalledWith(7, 2);
  });

  it("keeps centred lines centred without an inline style the CSP would drop", async () => {
    stubLocalStorage();
    renderReader();
    const line = await screen.findByText("* * *");
    expect(line).not.toHaveAttribute("style");
    expect(line).toHaveClass("fanf-align-center");
  });

  it("moves to the next and previous chapter, and stops at the ends", async () => {
    stubLocalStorage();
    const user = userEvent.setup();
    renderReader(3);
    await screen.findByRole("heading", { level: 1, name: "Chapter 3: Title 3" });

    expect(screen.getByRole("button", { name: "Next chapter" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Previous chapter" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Chapter 2: Title 2" })
    ).toBeInTheDocument();
  });

  it("jumps to a chapter from the chapter list", async () => {
    stubLocalStorage();
    const user = userEvent.setup();
    renderReader();
    await screen.findByRole("heading", { level: 1, name: "Chapter 2: Title 2" });

    await user.click(screen.getByRole("button", { name: "Chapter list" }));
    const list = await screen.findByRole("dialog", { name: "A Story" });
    await user.click(within(list).getByRole("button", { name: /Title 3/ }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Chapter 3: Title 3" })
    ).toBeInTheDocument();
  });

  it("bookmarks the story and shows it pressed", async () => {
    stubLocalStorage();
    const user = userEvent.setup();
    renderReader();
    const bookmark = await screen.findByRole("button", { name: "Bookmark" });
    expect(bookmark).toHaveAttribute("aria-pressed", "false");

    await user.click(bookmark);

    expect(api.toggleBookmarked).toHaveBeenCalledWith(7, true);
    await waitFor(() => expect(bookmark).toHaveAttribute("aria-pressed", "true"));
  });

  it("resumes the chapter last read at its saved position", async () => {
    stubLocalStorage();
    // A 1000px scroll range for every element: jsdom lays nothing out.
    vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(1100);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(100);
    try {
      renderReader(2); // STORY.lastChapterNum 2, lastScrollPct 40
      await screen.findByRole("heading", { level: 1, name: "Chapter 2: Title 2" });
      await waitFor(() => expect(screen.getByRole("main").scrollTop).toBe(400));
    } finally {
      vi.restoreAllMocks();
    }
  });

  it("goes Back and saves the position the reader scrolled to", async () => {
    stubLocalStorage();
    const user = userEvent.setup();
    renderReader();
    await screen.findByRole("heading", { level: 1, name: "Chapter 2: Title 2" });
    vi.mocked(api.setResumeChapter).mockClear();

    const scroller = screen.getByRole("main");
    Object.defineProperty(scroller, "scrollHeight", { configurable: true, value: 1100 });
    Object.defineProperty(scroller, "clientHeight", { configurable: true, value: 100 });
    scroller.scrollTop = 250;
    act(() => void scroller.dispatchEvent(new Event("scroll")));
    await user.click(screen.getByRole("button", { name: "Back" }));

    expect(onBack).toHaveBeenCalled();
    cleanup(); // the list screen replaces the Reader
    await waitFor(() => expect(api.setResumeChapter).toHaveBeenCalledWith(7, 2, 25));
  });
});

describe("reading settings", () => {
  async function openSettings() {
    const user = userEvent.setup();
    renderReader();
    await screen.findByRole("heading", { level: 1, name: "Chapter 2: Title 2" });
    await user.click(screen.getByRole("button", { name: "Reading settings" }));
    return { user, panel: await screen.findByRole("dialog", { name: "Reading settings" }) };
  }
  const field = (panel: HTMLElement, name: string) =>
    within(panel).getByRole("textbox", { name }) as HTMLInputElement;

  const noField = (panel: HTMLElement, name: string) =>
    expect(within(panel).queryByRole("textbox", { name })).not.toBeInTheDocument();

  it("shows the contract defaults when nothing is saved, on Serif at its own settings", async () => {
    stubLocalStorage();
    const { panel } = await openSettings();
    expect(within(panel).getByRole("radio", { name: "Serif" })).toBeChecked();
    expect(field(panel, "Size").value).toBe("24");
    expect(field(panel, "Line height").value).toBe("1.45");
    expect(field(panel, "Page width").value).toBe("58");
    noField(panel, "Weight");
    noField(panel, "Tracking");
    noField(panel, "Custom font");
    expect(prose().style.fontSize).toBe("24px");
    expect(prose().style.fontWeight).toBe("400");
  });

  it("opens a pre-adoption custom font on Custom, with its saved values", async () => {
    stubLocalStorage({
      fontFamily: "serif",
      customFont: "Literata",
      fontSize: 28,
      lineHeight: 1.35,
      fontWeight: 500,
      letterSpacing: 0.01,
    });
    const { panel } = await openSettings();
    expect(within(panel).getByRole("radio", { name: "Custom" })).toBeChecked();
    expect(field(panel, "Custom font").value).toBe("Literata");
    expect(field(panel, "Size").value).toBe("28");
    expect(field(panel, "Line height").value).toBe("1.35");
    expect(field(panel, "Weight").value).toBe("500");
    expect(field(panel, "Tracking").value).toBe("0.01");
    expect(prose().style.fontSize).toBe("28px");
    expect(prose().style.lineHeight).toBe("1.35");
    expect(prose().style.fontFamily).toBe('"Literata", var(--lk-font-reading-serif)');
  });

  it("switches to Work Sans at its own weight and tracking", async () => {
    stubLocalStorage();
    const { user, panel } = await openSettings();
    await user.click(within(panel).getByRole("radio", { name: "Sans" }));

    expect(prose().style.fontFamily).toBe("var(--lk-font-reading-sans)");
    expect(prose().style.fontWeight).toBe("430");
    expect(prose().style.letterSpacing).toBe("-0.02em");
    noField(panel, "Weight");
  });

  it("saves a changed size, leaving weight and tracking unset", async () => {
    const store = stubLocalStorage();
    const { user, panel } = await openSettings();
    await user.click(field(panel, "Size"));
    await user.keyboard("{ArrowUp}");

    expect(prose().style.fontSize).toBe("25px");
    expect(savedPrefs(store)).toMatchObject({
      fontSize: 25,
      fontWeight: null,
      letterSpacing: null,
    });
  });

  it("resets size, line height, bold weight and page width on a face", async () => {
    const store = stubLocalStorage({
      schema: 2,
      fontFamily: "sans",
      fontSize: 28,
      lineHeight: 1.35,
      fontWeightBold: 800,
      pageWidth: 90,
    });
    const { user, panel } = await openSettings();
    await user.click(within(panel).getByRole("button", { name: "Reset to defaults" }));

    expect(field(panel, "Size").value).toBe("24");
    expect(field(panel, "Line height").value).toBe("1.45");
    expect(field(panel, "Bold weight").value).toBe("700");
    expect(field(panel, "Page width").value).toBe("58");
    expect(savedPrefs(store)).toMatchObject({ fontFamily: "sans", fontSize: 24, pageWidth: 58 });
  });

  it("offers no reset on Custom", async () => {
    stubLocalStorage({ schema: 2, fontFamily: "custom", customFont: "Literata" });
    const { panel } = await openSettings();
    expect(
      within(panel).queryByRole("button", { name: "Reset to defaults" })
    ).not.toBeInTheDocument();
  });
});
