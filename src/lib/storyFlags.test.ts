import { beforeEach, describe, expect, it, vi } from "vitest";
import type { StoryRow } from "./types";

vi.mock("./api", () => ({
  toggleRead: vi.fn(),
  toggleDismissed: vi.fn(),
  toggleBookmarked: vi.fn()
}));

import * as api from "./api";
import { toggleStoryFlag } from "./storyFlags";

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
  ffnUpdatedAt: null,
  source: "ffn",
  cachedChapterCount: 0
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.toggleRead).mockResolvedValue(undefined);
  vi.mocked(api.toggleDismissed).mockResolvedValue(undefined);
  vi.mocked(api.toggleBookmarked).mockResolvedValue(undefined);
});

describe("toggleStoryFlag", () => {
  it("calls toggleRead with the inverted value and returns the patched story", async () => {
    const result = await toggleStoryFlag(story, "isRead");
    expect(api.toggleRead).toHaveBeenCalledWith(story.id, true);
    expect(result.isRead).toBe(true);
    expect(result).not.toBe(story);
  });

  it("calls toggleDismissed with the inverted value and returns the patched story", async () => {
    const result = await toggleStoryFlag(story, "dismissed");
    expect(api.toggleDismissed).toHaveBeenCalledWith(story.id, true);
    expect(result.dismissed).toBe(true);
  });

  it("calls toggleBookmarked with the inverted value and returns the patched story", async () => {
    const result = await toggleStoryFlag(story, "bookmarked");
    expect(api.toggleBookmarked).toHaveBeenCalledWith(story.id, true);
    expect(result.bookmarked).toBe(true);
  });

  it("flips back to false on a story where the flag is already true", async () => {
    const result = await toggleStoryFlag({ ...story, bookmarked: true }, "bookmarked");
    expect(api.toggleBookmarked).toHaveBeenCalledWith(story.id, false);
    expect(result.bookmarked).toBe(false);
  });

  it("propagates a rejection instead of swallowing it", async () => {
    vi.mocked(api.toggleRead).mockRejectedValueOnce(new Error("db locked"));
    await expect(toggleStoryFlag(story, "isRead")).rejects.toThrow("db locked");
  });
});
