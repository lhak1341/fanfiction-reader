import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fmtCompact, shortDate } from "./format";

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

describe("fmtCompact", () => {
  it("leaves counts under 1000 as plain digits", () => {
    expect(fmtCompact(0)).toBe("0");
    expect(fmtCompact(999)).toBe("999");
  });

  it("abbreviates thousands to one decimal, dropping a trailing .0", () => {
    expect(fmtCompact(1000)).toBe("1K");
    expect(fmtCompact(1250)).toBe("1.3K");
    expect(fmtCompact(12_340)).toBe("12.3K");
  });

  it("switches to M where the K form would round up to 1000K", () => {
    expect(fmtCompact(999_949)).toBe("999.9K");
    expect(fmtCompact(999_950)).toBe("1M");
    expect(fmtCompact(1_000_000)).toBe("1M");
    expect(fmtCompact(1_250_000)).toBe("1.3M");
  });
});
