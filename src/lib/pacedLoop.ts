// One paced/best-effort background loop used by every bulk fetch in the app
// (bookmark refresh, chapter download, category indexing) — architecture
// review candidate 2, 2026-07-21. `prefetchNeighbors` (Reader.svelte) stays
// separate: it cancels mid-loop via a generation token and skips pacing
// entirely on an already-cached chapter, a different shape than the
// run-to-completion loops here — but it imports DEFAULT_JITTER_MS below so
// the pacing number itself still lives in one place.

// A burst of requests with no gaps reads as bot traffic to Cloudflare in a
// way a single fetch doesn't (confirmed live, SPEC.md §3/§8) — every paced
// loop in the app waits somewhere in this range between items.
export const DEFAULT_JITTER_MS = { min: 1500, max: 3000 };

function jitterDelay(jitter: typeof DEFAULT_JITTER_MS): number {
  return jitter.min + Math.random() * (jitter.max - jitter.min);
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

/// Runs `fn` over `items` sequentially, pacing between calls (never after
/// the last) and never stopping early on failure — one item throwing just
/// means that item's `result` is the `Error`, the rest still run.
export async function runPacedBestEffort<T, R>(
  items: readonly T[],
  fn: (item: T, index: number) => Promise<R>,
  onProgress?: (index: number, total: number, item: T, result: R | Error) => void,
  jitter: typeof DEFAULT_JITTER_MS = DEFAULT_JITTER_MS
): Promise<void> {
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    try {
      const result = await fn(item, i);
      onProgress?.(i, items.length, item, result);
    } catch (e) {
      onProgress?.(i, items.length, item, e instanceof Error ? e : new Error(String(e)));
    }
    if (i < items.length - 1) {
      await sleep(jitterDelay(jitter));
    }
  }
}
