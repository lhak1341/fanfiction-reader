import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// The on<X>Fetched/on<X>FetchError listener pair behind every fetch kind in
// api.ts (chapter/browse/fandom-index) — architecture review candidate 3,
// 2026-07-23. Each fetch kind's own trigger-and-wait shape stays hand-written
// in api.ts (`awaitCorrelatedEvent` below) rather than folded in here: chapter
// fetch needs a `match` predicate and picks between two trigger commands by
// source, browse/fandom-index need neither — real per-case variance, not
// boilerplate.
export function defineFetchEvent<T>(okEvent: string, errEvent: string) {
  return {
    on: (handler: (e: T) => void): Promise<UnlistenFn> => listen<T>(okEvent, (e) => handler(e.payload)),
    onError: (handler: (message: string) => void): Promise<UnlistenFn> => listen<string>(errEvent, (e) => handler(e.payload))
  };
}

// Fires `trigger`, then waits for whichever of `listenOk`/`listenErr` fires
// first (filtered through `match`, when given — otherwise the very next
// success event resolves it). Both listeners are torn down the moment
// either settles. This is the shared shape behind every "invoke a Tauri
// command, then wait for its correlated event" call in api.ts.
export function awaitCorrelatedEvent<T>(
  trigger: () => Promise<unknown>,
  listenOk: (handler: (e: T) => void) => Promise<UnlistenFn>,
  listenErr: (handler: (msg: string) => void) => Promise<UnlistenFn>,
  match?: (e: T) => boolean
): Promise<T> {
  return new Promise((resolve, reject) => {
    let unlistenOk: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    const cleanup = () => {
      unlistenOk?.();
      unlistenErr?.();
    };
    listenOk((e) => {
      if (match && !match(e)) return;
      cleanup();
      resolve(e);
    }).then((fn) => (unlistenOk = fn));
    listenErr((msg) => {
      cleanup();
      reject(new Error(msg));
    }).then((fn) => (unlistenErr = fn));
    trigger().catch((e) => {
      cleanup();
      reject(e);
    });
  });
}
