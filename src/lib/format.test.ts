import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { shortDate } from "./format";

describe("shortDate", () => {
  it("passes a relative age straight through", () => {
    expect(shortDate("2h")).toBe("2h");
    expect(shortDate("14d")).toBe("14d");
  });

  it("compacts a full M/D/YYYY date", () => {
    expect(shortDate("3/15/2023")).toBe("23-03-15");
  });

  it("returns null for text that matches none of the three shapes", () => {
    expect(shortDate("banana")).toBeNull();
    expect(shortDate("")).toBeNull();
  });

  describe("bare M/D (year-inferred)", () => {
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(2026, 6, 19)); // "now" = July 19, 2026
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("resolves to this year when the date already occurred", () => {
      expect(shortDate("6/20")).toBe("26-06-20");
    });

    it("rolls back to last year when the date would otherwise land in the future", () => {
      expect(shortDate("12/30")).toBe("25-12-30");
    });
  });
});
