import { describe, expect, it, vi, beforeEach } from "vitest";
import type { StoryRow } from "./types";

vi.mock("./storyFlags", () => ({
  toggleStoryFlag: vi.fn()
}));

import { toggleStoryFlag } from "./storyFlags";
import { toggleReadAndRefresh, toggleDismissedAndRefresh, toggleBookmarkedAndRefresh } from "./storyFlagActions";

const story: StoryRow = {
  id: 1,
  sourceStoryId: 123,
  title: "t",
  author: "a",
  fandom: "f",
  rating: "K",
  genre: null,
  language: null,
  tags: null,
  status: "in_progress",
  wordCount: 0,
  favoriteCount: 0,
  chapterCount: 5,
  summary: null,
  isRead: false,
  dismissed: false,
  bookmarked: false,
  lastChapterNum: null,
  lastScrollPct: null,
  ffnUpdatedAt: null,
  source: "ffn",
  cachedChapterCount: 0
};

describe("storyFlagActions", () => {
  beforeEach(() => {
    vi.mocked(toggleStoryFlag).mockReset();
  });

  it("toggleReadAndRefresh calls toggleStoryFlag(isRead) then refreshAll", async () => {
    vi.mocked(toggleStoryFlag).mockResolvedValue(story);
    const onError = vi.fn();
    const refreshAll = vi.fn().mockResolvedValue(undefined);

    await toggleReadAndRefresh(story, onError, refreshAll);

    expect(toggleStoryFlag).toHaveBeenCalledWith(story, "isRead");
    expect(refreshAll).toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it("toggleDismissedAndRefresh calls toggleStoryFlag(dismissed) then refreshAll", async () => {
    vi.mocked(toggleStoryFlag).mockResolvedValue(story);
    const onError = vi.fn();
    const refreshAll = vi.fn().mockResolvedValue(undefined);

    await toggleDismissedAndRefresh(story, onError, refreshAll);

    expect(toggleStoryFlag).toHaveBeenCalledWith(story, "dismissed");
    expect(refreshAll).toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it("toggleBookmarkedAndRefresh calls toggleStoryFlag(bookmarked) then refreshAll", async () => {
    vi.mocked(toggleStoryFlag).mockResolvedValue(story);
    const onError = vi.fn();
    const refreshAll = vi.fn().mockResolvedValue(undefined);

    await toggleBookmarkedAndRefresh(story, onError, refreshAll);

    expect(toggleStoryFlag).toHaveBeenCalledWith(story, "bookmarked");
    expect(refreshAll).toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it("reports the read-specific message and skips refreshAll on failure", async () => {
    vi.mocked(toggleStoryFlag).mockRejectedValue(new Error("db locked"));
    const onError = vi.fn();
    const refreshAll = vi.fn().mockResolvedValue(undefined);

    await toggleReadAndRefresh(story, onError, refreshAll);

    expect(onError).toHaveBeenCalledWith("Could not update read state: Error: db locked");
    expect(refreshAll).not.toHaveBeenCalled();
  });

  it("reports the dismissed-specific message on failure", async () => {
    vi.mocked(toggleStoryFlag).mockRejectedValue(new Error("db locked"));
    const onError = vi.fn();
    const refreshAll = vi.fn().mockResolvedValue(undefined);

    await toggleDismissedAndRefresh(story, onError, refreshAll);

    expect(onError).toHaveBeenCalledWith("Could not update dismissed state: Error: db locked");
  });

  it("reports the bookmark-specific message on failure", async () => {
    vi.mocked(toggleStoryFlag).mockRejectedValue(new Error("db locked"));
    const onError = vi.fn();
    const refreshAll = vi.fn().mockResolvedValue(undefined);

    await toggleBookmarkedAndRefresh(story, onError, refreshAll);

    expect(onError).toHaveBeenCalledWith("Could not update bookmark: Error: db locked");
  });
});
