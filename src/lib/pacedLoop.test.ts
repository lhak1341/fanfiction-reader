import { describe, expect, it, vi } from "vitest";
import {
  DEFAULT_JITTER_MS,
  DEFAULT_RETRY_PAUSE_MS,
  describeFailures,
  jitterDelay,
  PauseGate,
  runPacedBestEffort,
} from "./pacedLoop";

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

    const run = runPacedBestEffort([1, 2, 3], fn, { onProgress });
    await vi.runAllTimersAsync();
    await run;

    expect(fn.mock.calls.map((c) => c[0])).toEqual([1, 2, 3]);
    expect(onProgress.mock.calls.map(([e]) => e)).toEqual([
      { round: 0, index: 0, total: 3, item: 1, result: 10 },
      { round: 0, index: 1, total: 3, item: 2, result: 20 },
      { round: 0, index: 2, total: 3, item: 3, result: 30 },
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

    const run = runPacedBestEffort([1, 2, 3], fn, { onProgress });
    await vi.runAllTimersAsync();
    await run;

    expect(fn).toHaveBeenCalledTimes(3);
    expect(onProgress.mock.calls[0][0]).toEqual({
      round: 0,
      index: 0,
      total: 3,
      item: 1,
      result: 10,
    });
    expect(onProgress.mock.calls[1][0].index).toBe(1);
    expect(onProgress.mock.calls[1][0].result).toBeInstanceOf(Error);
    expect((onProgress.mock.calls[1][0].result as Error).message).toBe("boom");
    expect(onProgress.mock.calls[2][0]).toEqual({
      round: 0,
      index: 2,
      total: 3,
      item: 3,
      result: 30,
    });
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

  it("holds the next item while the gate is paused and continues on resume", async () => {
    vi.useFakeTimers();
    const gate = new PauseGate();
    const fn = vi.fn(async () => undefined);

    const run = runPacedBestEffort([1, 2, 3], fn, { gate });

    await vi.advanceTimersByTimeAsync(0);
    expect(fn).toHaveBeenCalledTimes(1);

    gate.pause();
    await vi.advanceTimersByTimeAsync(DEFAULT_JITTER_MS.max * 5);
    expect(fn).toHaveBeenCalledTimes(1);

    gate.resume();
    await vi.advanceTimersByTimeAsync(DEFAULT_JITTER_MS.max * 3);
    await run;
    expect(fn).toHaveBeenCalledTimes(3);
    vi.useRealTimers();
  });

  it("does not start the first item while the gate is already paused", async () => {
    vi.useFakeTimers();
    const gate = new PauseGate();
    gate.pause();
    const fn = vi.fn(async () => undefined);

    const run = runPacedBestEffort([1], fn, { gate });
    await vi.advanceTimersByTimeAsync(DEFAULT_JITTER_MS.max * 5);
    expect(fn).not.toHaveBeenCalled();

    gate.resume();
    await run;
    expect(fn).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});

describe("runPacedBestEffort results and retries", () => {
  it("returns ok and failed items with the failing reason", async () => {
    vi.useFakeTimers();
    const run = runPacedBestEffort([1, 2, 3], async (n) => {
      if (n === 2) throw new Error("boom");
    });
    await vi.runAllTimersAsync();

    expect(await run).toEqual({ ok: [1, 3], failed: [{ item: 2, reason: "boom" }] });
    vi.useRealTimers();
  });

  it("does not retry by default", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(async () => {
      throw new Error("no");
    });
    const run = runPacedBestEffort([1], fn);
    await vi.runAllTimersAsync();
    await run;

    expect(fn).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it("retries failures after the retry pause, reports the round, and moves a recovered item to ok", async () => {
    vi.useFakeTimers();
    let calls = 0;
    const fn = vi.fn(async (n: number) => {
      if (n === 1 && ++calls === 1) throw new Error("Cloudflare");
    });
    const onProgress = vi.fn();

    // Math.random() = 0 pins jitter to its minimum: item 2 starts at
    // 1500 ms, the retry pass at 1500 + DEFAULT_RETRY_PAUSE_MS.
    vi.spyOn(Math, "random").mockReturnValue(0);

    const run = runPacedBestEffort([1, 2], fn, { retryRounds: 1, onProgress });
    await vi.advanceTimersByTimeAsync(DEFAULT_JITTER_MS.min);
    expect(fn).toHaveBeenCalledTimes(2);

    // Not retried before the retry pause has passed.
    await vi.advanceTimersByTimeAsync(DEFAULT_RETRY_PAUSE_MS - 1);
    expect(fn).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(1);
    const result = await run;

    expect(fn.mock.calls.map((c) => c[0])).toEqual([1, 2, 1]);
    expect(result).toEqual({ ok: [2, 1], failed: [] });
    expect(onProgress.mock.calls.map(([e]) => [e.round, e.item])).toEqual([
      [0, 1],
      [0, 2],
      [1, 1],
    ]);
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("keeps the last reason for an item that fails every round", async () => {
    vi.useFakeTimers();
    let n = 0;
    const run = runPacedBestEffort(
      [1],
      async () => {
        throw new Error(`attempt ${++n}`);
      },
      { retryRounds: 2 }
    );
    await vi.runAllTimersAsync();

    expect((await run).failed).toEqual([{ item: 1, reason: "attempt 3" }]);
    vi.useRealTimers();
  });

  it("holds a retry round at the gate too", async () => {
    vi.useFakeTimers();
    const gate = new PauseGate();
    const fn = vi.fn(async () => {
      throw new Error("x");
    });
    const run = runPacedBestEffort([1], fn, { retryRounds: 1, gate });
    await vi.advanceTimersByTimeAsync(0);
    expect(fn).toHaveBeenCalledTimes(1);

    gate.pause();
    await vi.advanceTimersByTimeAsync(DEFAULT_RETRY_PAUSE_MS * 2);
    expect(fn).toHaveBeenCalledTimes(1);

    gate.resume();
    await vi.runAllTimersAsync();
    await run;
    expect(fn).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });
});

describe("describeFailures", () => {
  const failed = [
    { item: 4, reason: "a" },
    { item: 9, reason: "b" },
  ];

  it("lists the failed items and the last reason", () => {
    expect(describeFailures(failed, (n) => n)).toBe("(4, 9). Last error: b");
  });

  it("caps the list at 8", () => {
    const many = Array.from({ length: 10 }, (_, i) => ({ item: i + 1, reason: "r" }));
    expect(describeFailures(many, (n) => n)).toBe("(1, 2, 3, 4, 5, 6, 7, 8…). Last error: r");
  });

  it("leaves the list out without a label", () => {
    expect(describeFailures(failed)).toBe("Last error: b");
  });
});

describe("PauseGate", () => {
  it("wait() resolves immediately when never paused", async () => {
    await expect(new PauseGate().wait()).resolves.toBeUndefined();
  });

  it("resume() releases every pending waiter", async () => {
    const gate = new PauseGate();
    gate.pause();
    const released = vi.fn();
    const a = gate.wait().then(released);
    const b = gate.wait().then(released);
    await Promise.resolve();
    expect(released).not.toHaveBeenCalled();

    gate.resume();
    await Promise.all([a, b]);
    expect(released).toHaveBeenCalledTimes(2);
  });
});
