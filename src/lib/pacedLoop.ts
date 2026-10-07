// One paced/best-effort background loop used by every bulk fetch in the app
// (bookmark refresh, chapter download, category indexing) — architecture
// review candidate 2, 2026-07-21. It also owns retry rounds and the failure
// list those callers report (architecture review, 2026-10-07): before that,
// only download retried and reported, and refresh/indexing failed silently. `prefetchNeighbors` (Reader.svelte) stays
// separate: it cancels mid-loop via a generation token and skips pacing
// entirely on an already-cached chapter, a different shape than the
// run-to-completion loops here — but it imports DEFAULT_JITTER_MS below so
// the pacing number itself still lives in one place.

// A burst of requests with no gaps reads as bot traffic to Cloudflare in a
// way a single fetch doesn't (confirmed live, SPEC.md §3/§8) — every paced
// loop in the app waits somewhere in this range between items.
export const DEFAULT_JITTER_MS = { min: 1500, max: 3000 };

// Architecture review candidate 5 (2026-07-21): exported so Reader.svelte's
// prefetchNeighbors (which can't reuse runPacedBestEffort itself — it needs
// mid-loop cancellation via a generation token and skips pacing entirely on
// an already-cached chapter) can still share this arithmetic instead of
// re-deriving it.
export function jitterDelay(jitter: typeof DEFAULT_JITTER_MS = DEFAULT_JITTER_MS): number {
  return jitter.min + Math.random() * (jitter.max - jitter.min);
}

export function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

/// A pause/resume switch a paced loop checks before each item. Open by
/// default; while paused, `wait()` stays pending until `resume()`. Lets a
/// background loop (bookmark refresh/download) stay out of the way of a
/// foreground fetch (Reader) without being cancelled — the item already in
/// flight when `pause()` lands still finishes (or is interrupted by the
/// foreground fetch, which wins under "newest wins").
export class PauseGate {
  private paused = false;
  private waiters: Array<() => void> = [];

  pause() {
    this.paused = true;
  }

  resume() {
    this.paused = false;
    const waiters = this.waiters;
    this.waiters = [];
    for (const wake of waiters) wake();
  }

  wait(): Promise<void> {
    return this.paused ? new Promise((resolve) => this.waiters.push(resolve)) : Promise.resolve();
  }
}

// Failed items get `retryRounds` extra passes, each after a longer pause and
// at wider pacing: a failure is usually Cloudflare unhappy with the cadence,
// so retrying at the same speed would just repeat it.
export const DEFAULT_RETRY_PAUSE_MS = 10_000;
export const DEFAULT_RETRY_JITTER_MS = { min: 4000, max: 7000 };

export interface PacedProgress<T, R> {
  /// 0 for the first pass, 1.. for retry passes.
  round: number;
  /// Position within this round's items, not the original list.
  index: number;
  total: number;
  item: T;
  result: R | Error;
}

export interface PacedOptions<T, R> {
  onProgress?: (event: PacedProgress<T, R>) => void;
  jitter?: typeof DEFAULT_JITTER_MS;
  /// Awaited before every item (including the first) — lets a `PauseGate`
  /// hold the loop.
  gate?: PauseGate;
  /// Extra passes over the items that failed. Default 0: report only.
  retryRounds?: number;
  retryPauseMs?: number;
  retryJitter?: typeof DEFAULT_JITTER_MS;
}

export interface PacedFailure<T> {
  item: T;
  /// The last attempt's error message.
  reason: string;
}

export interface PacedResult<T> {
  ok: T[];
  failed: PacedFailure<T>[];
}

/// Runs `fn` over `items` sequentially, pacing between calls (never after
/// the last of a pass) and never stopping early on failure — a throwing item
/// is recorded and the rest still run. Failures are retried for
/// `retryRounds` further passes; whatever still fails comes back in
/// `failed`, so every caller can report it the same way (`describeFailures`).
export async function runPacedBestEffort<T, R>(
  items: readonly T[],
  fn: (item: T, index: number) => Promise<R>,
  options: PacedOptions<T, R> = {}
): Promise<PacedResult<T>> {
  const {
    onProgress,
    jitter = DEFAULT_JITTER_MS,
    gate,
    retryRounds = 0,
    retryPauseMs = DEFAULT_RETRY_PAUSE_MS,
    retryJitter = DEFAULT_RETRY_JITTER_MS
  } = options;
  const ok: T[] = [];
  let pending = [...items];
  let failed: PacedFailure<T>[] = [];

  for (let round = 0; round <= retryRounds && pending.length > 0; round++) {
    if (round > 0) await sleep(retryPauseMs);
    failed = [];
    for (let i = 0; i < pending.length; i++) {
      const item = pending[i];
      await gate?.wait();
      let result: R | Error;
      try {
        result = await fn(item, i);
        ok.push(item);
      } catch (e) {
        result = e instanceof Error ? e : new Error(String(e));
        failed.push({ item, reason: result.message });
      }
      onProgress?.({ round, index: i, total: pending.length, item, result });
      if (i < pending.length - 1) {
        await sleep(jitterDelay(round === 0 ? jitter : retryJitter));
      }
    }
    pending = failed.map((f) => f.item);
  }
  return { ok, failed };
}

/// "(4, 9, 20…). Last error: reason" — the part of a failure message every
/// bulk-fetch caller shares; callers add the count noun in front and any
/// advice after. `label` names each failed item in the list (capped at 8);
/// omit it to leave the list out.
export function describeFailures<T>(failed: readonly PacedFailure<T>[], label?: (item: T) => string | number): string {
  const last = failed[failed.length - 1]?.reason ?? "unknown";
  if (!label) return `Last error: ${last}`;
  const names = failed.map((f) => label(f.item));
  const list = names.length > 8 ? `${names.slice(0, 8).join(", ")}…` : names.join(", ");
  return `(${list}). Last error: ${last}`;
}
