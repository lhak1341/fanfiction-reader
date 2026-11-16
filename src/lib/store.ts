// The change signal every *Screen class carries, framework-free (it replaced
// Svelte runes with the React rewrite). A `Store` subclass declares plain
// fields; assigning one tells subscribers (React reads them through
// `useSyncExternalStore(store.subscribe, store.getVersion)`, see
// `src/app/useStore.ts`) and queues the reactions watching that field.
//
// Reactions keep the old `$effect` timing: they run together on the next
// microtask, or at once on `flushSync()` (what tests call after assigning a
// field). A reaction queued twice before the flush runs once, and assigning a
// field the value it already holds (`Object.is`) changes nothing.
//
// Only assigning a field counts, as with `$state` on a Set: mutate a copy and
// assign it (`this.stories = [...]`), never `this.stories.push()`.

export interface Subscribable {
  subscribe: (listener: () => void) => () => void;
  getVersion: () => number;
}

type Reaction = () => void;

const queue = new Set<Reaction>();
let scheduled = false;

// A reaction that keeps changing a field it watches would requeue itself forever.
const MAX_FLUSH_ROUNDS = 100;

/// Runs every queued reaction now, including the ones they queue in turn.
export function flushSync(): void {
  for (let round = 0; queue.size > 0; round++) {
    if (round === MAX_FLUSH_ROUNDS) {
      queue.clear();
      throw new Error("store: reactions kept requeueing each other");
    }
    const runs = [...queue];
    queue.clear();
    for (const run of runs) run();
  }
}

function schedule(run: Reaction) {
  queue.add(run);
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(() => {
    scheduled = false;
    flushSync();
  });
}

function isAccessor(object: object, key: PropertyKey): boolean {
  for (let o: object | null = object; o !== null; o = Object.getPrototypeOf(o)) {
    const desc = Object.getOwnPropertyDescriptor(o, key);
    if (desc) return !("value" in desc);
  }
  return false;
}

interface Hub {
  listeners: Set<() => void>;
  version: number;
  watchers: Map<PropertyKey, Set<Reaction>>;
}

export abstract class Store implements Subscribable {
  // Set before the constructor hands out the proxy, so it never goes through the trap.
  private readonly hub: Hub = { listeners: new Set(), version: 0, watchers: new Map() };

  readonly subscribe = (listener: () => void): (() => void) => {
    this.hub.listeners.add(listener);
    return () => {
      this.hub.listeners.delete(listener);
    };
  };

  readonly getVersion = (): number => this.hub.version;

  constructor() {
    // `this` in a subclass is what the base constructor returns, so every
    // assignment goes through this proxy. Field initialisers define their
    // fields instead (ES2022 class fields), so they notify nobody.
    return new Proxy(this, {
      set(target, key, value, receiver) {
        // An accessor (here or on a prototype): its setter assigns fields through the proxy.
        if (isAccessor(target, key)) return Reflect.set(target, key, value, receiver);
        const own = Object.getOwnPropertyDescriptor(target, key);
        if (own !== undefined && Object.is(own.value, value)) return true;
        Reflect.set(target, key, value);
        const hub = target.hub;
        hub.version += 1;
        for (const run of hub.watchers.get(key) ?? []) schedule(run);
        for (const listener of [...hub.listeners]) listener();
        return true;
      },
    });
  }

  /// Runs `run` on the next flush, then again after any of `keys` changes,
  /// until the returned stop is called. The explicit key list is the
  /// dependency list `$effect` used to infer from what it read.
  protected watch(keys: readonly (keyof this)[], run: Reaction): () => void {
    const { watchers } = this.hub;
    for (const key of keys) {
      let set = watchers.get(key);
      if (!set) watchers.set(key, (set = new Set()));
      set.add(run);
    }
    schedule(run);
    return () => {
      for (const key of keys) watchers.get(key)?.delete(run);
      queue.delete(run);
    };
  }
}
