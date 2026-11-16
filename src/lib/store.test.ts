// The part of the Store contract the *Screen tests don't reach: what a view
// subscribed through `useStore` sees. (The reactions are pinned by the
// Screen tests' start/dispose cases.)
import { describe, expect, it, vi } from "vitest";
import { flushSync, Store } from "./store";

class Counter extends Store {
  count = 0;
  items: string[] = [];
  runs = 0;

  start() {
    return this.watch(["count"], () => {
      this.runs += 1;
    });
  }
}

describe("Store", () => {
  it("tells subscribers about each assignment that changes a field", () => {
    const counter = new Counter();
    const listener = vi.fn();
    counter.subscribe(listener);
    const before = counter.getVersion();

    counter.count = 1;
    counter.items = ["a"];

    expect(listener).toHaveBeenCalledTimes(2);
    expect(counter.getVersion()).toBe(before + 2);
  });

  it("ignores an assignment of the value a field already holds", () => {
    const counter = new Counter();
    const listener = vi.fn();
    counter.subscribe(listener);

    counter.count = 0;

    expect(listener).not.toHaveBeenCalled();
  });

  it("stops telling a subscriber once it unsubscribes", () => {
    const counter = new Counter();
    const listener = vi.fn();
    const unsubscribe = counter.subscribe(listener);
    unsubscribe();

    counter.count = 1;

    expect(listener).not.toHaveBeenCalled();
  });

  it("runs a reaction once for several changes before the flush", () => {
    const counter = new Counter();
    counter.start();
    flushSync();
    expect(counter.runs).toBe(1);

    counter.count = 1;
    counter.count = 2;
    flushSync();

    expect(counter.runs).toBe(2);
  });

  it("runs queued reactions on the next microtask without a flush", async () => {
    const counter = new Counter();
    counter.start();
    await Promise.resolve();
    expect(counter.runs).toBe(1);
  });

  it("never runs a reaction stopped before its queued run", () => {
    const counter = new Counter();
    const stop = counter.start();
    stop();
    flushSync();
    expect(counter.runs).toBe(0);
  });
});
