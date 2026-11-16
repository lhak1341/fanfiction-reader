import { useSyncExternalStore } from "react";
import type { Subscribable } from "$lib/store";

/// Re-renders the calling component whenever a `Store` changes, and hands the
/// store back to read from.
export function useStore<T extends Subscribable>(store: T): T {
  useSyncExternalStore(store.subscribe, store.getVersion);
  return store;
}
