import { describe, expect, it, vi } from "vitest";
import { awaitCorrelatedEvent } from "./tauriEvents";

// awaitCorrelatedEvent takes listenOk/listenErr as plain params (dependency
// injection), so these fakes stand in for the real Tauri `listen()` without
// mocking @tauri-apps/api/event. `listen()` unlisten-fn assignment happens
// via a `.then()` microtask, so tests flush one microtask tick (`await
// Promise.resolve()`) before firing an event, matching real Tauri ordering
// (an event can't arrive before its own registration promise resolves).
function fakeListener<E>() {
  let handler: ((e: E) => void) | undefined;
  const unlisten = vi.fn();
  const listen = (h: (e: E) => void) => {
    handler = h;
    return Promise.resolve(unlisten);
  };
  const fire = (e: E) => handler?.(e);
  return { listen, fire, unlisten };
}

describe("awaitCorrelatedEvent", () => {
  it("resolves with the ok event's payload", async () => {
    const ok = fakeListener<{ storyId: number }>();
    const err = fakeListener<string>();
    const trigger = vi.fn().mockResolvedValue(undefined);

    const promise = awaitCorrelatedEvent(trigger, ok.listen, err.listen);
    await Promise.resolve();
    ok.fire({ storyId: 1 });

    await expect(promise).resolves.toEqual({ storyId: 1 });
  });

  it("rejects with an Error when the err event fires", async () => {
    const ok = fakeListener<{ storyId: number }>();
    const err = fakeListener<string>();
    const trigger = vi.fn().mockResolvedValue(undefined);

    const promise = awaitCorrelatedEvent(trigger, ok.listen, err.listen);
    await Promise.resolve();
    err.fire("db locked");

    await expect(promise).rejects.toThrow("db locked");
  });

  it("rejects and cleans up when trigger() itself rejects", async () => {
    const ok = fakeListener<{ storyId: number }>();
    const err = fakeListener<string>();
    const trigger = vi.fn().mockRejectedValue(new Error("invoke failed"));

    const promise = awaitCorrelatedEvent(trigger, ok.listen, err.listen);

    await expect(promise).rejects.toThrow("invoke failed");
    expect(ok.unlisten).toHaveBeenCalledTimes(1);
    expect(err.unlisten).toHaveBeenCalledTimes(1);
  });

  it("ignores ok events that don't match, resolves once one does", async () => {
    const ok = fakeListener<{ storyId: number }>();
    const err = fakeListener<string>();
    const trigger = vi.fn().mockResolvedValue(undefined);

    const promise = awaitCorrelatedEvent(trigger, ok.listen, err.listen, (e) => e.storyId === 2);
    await Promise.resolve();
    ok.fire({ storyId: 1 });
    ok.fire({ storyId: 2 });

    await expect(promise).resolves.toEqual({ storyId: 2 });
  });

  it("calls both unlisten functions exactly once once settled", async () => {
    const ok = fakeListener<{ storyId: number }>();
    const err = fakeListener<string>();
    const trigger = vi.fn().mockResolvedValue(undefined);

    const promise = awaitCorrelatedEvent(trigger, ok.listen, err.listen);
    await Promise.resolve();
    ok.fire({ storyId: 1 });
    await promise;

    expect(ok.unlisten).toHaveBeenCalledTimes(1);
    expect(err.unlisten).toHaveBeenCalledTimes(1);
  });
});
