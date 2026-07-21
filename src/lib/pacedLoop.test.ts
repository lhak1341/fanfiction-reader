import { describe, expect, it, vi } from "vitest";
import { DEFAULT_JITTER_MS, jitterDelay, runPacedBestEffort } from "./pacedLoop";

describe("jitterDelay", () => {
  // Architecture review candidate 5: exported so Reader.svelte's
  // prefetchNeighbors can share this arithmetic instead of re-deriving it —
  // worth locking in the bounds directly now that two call sites depend on it.
  it("defaults to a value within DEFAULT_JITTER_MS's range", () => {
    for (let i = 0; i < 50; i++) {
      const ms = jitterDelay();
      expect(ms).toBeGreaterThanOrEqual(DEFAULT_JITTER_MS.min);
      expect(ms).toBeLessThan(DEFAULT_JITTER_MS.max);
    }
  });

  it("respects a custom jitter range", () => {
    const custom = { min: 100, max: 200 };
    for (let i = 0; i < 50; i++) {
      const ms = jitterDelay(custom);
      expect(ms).toBeGreaterThanOrEqual(custom.min);
      expect(ms).toBeLessThan(custom.max);
    }
  });
});

describe("runPacedBestEffort", () => {
  it("calls fn for every item, in order, passing through the result", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(async (item: number) => item * 10);
    const onProgress = vi.fn();

    const run = runPacedBestEffort([1, 2, 3], fn, onProgress);
    await vi.runAllTimersAsync();
    await run;

    expect(fn.mock.calls.map((c) => c[0])).toEqual([1, 2, 3]);
    expect(onProgress.mock.calls).toEqual([
      [0, 3, 1, 10],
      [1, 3, 2, 20],
      [2, 3, 3, 30]
    ]);
    vi.useRealTimers();
  });

  it("keeps going after a failure, passing the Error to onProgress instead of stopping", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(async (item: number) => {
      if (item === 2) throw new Error("boom");
      return item * 10;
    });
    const onProgress = vi.fn();

    const run = runPacedBestEffort([1, 2, 3], fn, onProgress);
    await vi.runAllTimersAsync();
    await run;

    expect(fn).toHaveBeenCalledTimes(3);
    expect(onProgress.mock.calls[0]).toEqual([0, 3, 1, 10]);
    expect(onProgress.mock.calls[1][0]).toBe(1);
    expect(onProgress.mock.calls[1][3]).toBeInstanceOf(Error);
    expect((onProgress.mock.calls[1][3] as Error).message).toBe("boom");
    expect(onProgress.mock.calls[2]).toEqual([2, 3, 3, 30]);
    vi.useRealTimers();
  });

  it("paces between items but not after the last one", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(async () => undefined);

    const run = runPacedBestEffort([1, 2, 3], fn);

    await vi.advanceTimersByTimeAsync(0);
    expect(fn).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(DEFAULT_JITTER_MS.max);
    expect(fn).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(DEFAULT_JITTER_MS.max);
    expect(fn).toHaveBeenCalledTimes(3);

    await run;
    vi.useRealTimers();
  });
});
