import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  searchFfnAndWait: vi.fn(),
  searchAo3AndWait: vi.fn(),
}));

import * as api from "./api";
import { saveAo3Tag } from "./fandom";
import { ficSourceAdapter } from "./ficSource";

beforeEach(() => {
  vi.clearAllMocks();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
  });
});

afterEach(() => vi.unstubAllGlobals());

describe("ficSourceAdapter(ffn).loadMore", () => {
  it("forwards the filters with no category (looked up server-side)", async () => {
    vi.mocked(api.searchFfnAndWait).mockResolvedValue({ fandom: "Naruto", count: 3 });

    const result = await ficSourceAdapter("ffn").loadMore("Naruto", 40000, "complete");

    expect(api.searchFfnAndWait).toHaveBeenCalledWith("Naruto", 40000, "complete");
    expect(result).toEqual({ fandom: "Naruto", count: 3 });
  });
});

describe("ficSourceAdapter(ao3).loadMore", () => {
  it("looks up the saved tag and force-applies the fandom label", async () => {
    saveAo3Tag("Naruto", "Naruto (Anime & Manga)");
    vi.mocked(api.searchAo3AndWait).mockResolvedValue({ fandom: "Naruto", count: 2 });

    await ficSourceAdapter("ao3").loadMore("Naruto", 1000, "any");

    expect(api.searchAo3AndWait).toHaveBeenCalledWith(
      "Naruto (Anime & Manga)",
      "Naruto",
      1000,
      "any"
    );
  });

  it("rejects without fetching when no tag was saved for the fandom", async () => {
    await expect(ficSourceAdapter("ao3").loadMore("Bleach", 0, "any")).rejects.toThrow(
      'No AO3 tag saved for "Bleach" yet — search it once via the modal first.'
    );
    expect(api.searchAo3AndWait).not.toHaveBeenCalled();
  });
});

describe("ficSourceAdapter", () => {
  it("picks the AO3 adapter for ao3 and the FFN adapter otherwise", async () => {
    saveAo3Tag("Naruto", "tag");
    vi.mocked(api.searchFfnAndWait).mockResolvedValue({ fandom: "Naruto", count: 0 });
    vi.mocked(api.searchAo3AndWait).mockResolvedValue({ fandom: "Naruto", count: 0 });

    await ficSourceAdapter("ffn").loadMore("Naruto", 0, "any");
    expect(api.searchFfnAndWait).toHaveBeenCalledTimes(1);
    expect(api.searchAo3AndWait).not.toHaveBeenCalled();

    await ficSourceAdapter("ao3").loadMore("Naruto", 0, "any");
    expect(api.searchAo3AndWait).toHaveBeenCalledTimes(1);
  });
});
