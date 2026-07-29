import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// The on<X>Fetched/on<X>FetchError listener pair behind every fetch kind in
// api.ts (chapter/browse/fandom-index) — architecture review candidate 3,
// 2026-07-23. Each fetch kind's own trigger-and-wait shape stays hand-written
// in api.ts (`awaitCorrelatedEvent` below) rather than folded in here: chapter
// fetch needs a `match` predicate and picks between two trigger commands by
// source, browse/fandom-index need neither — real per-case variance, not
// boilerplate.
export function defineFetchEvent<TOk, TErr>(okEvent: string, errEvent: string) {
  return {
    on: (handler: (e: TOk) => void): Promise<UnlistenFn> => listen<TOk>(okEvent, (e) => handler(e.payload)),
    onError: (handler: (e: TErr) => void): Promise<UnlistenFn> => listen<TErr>(errEvent, (e) => handler(e.payload))
  };
}

// Fires `trigger`, then waits for whichever of `listenOk`/`listenErr` fires
// first (filtered through `match`, when given — otherwise the very next
// event of either kind resolves/rejects it). `match` runs against both the
// success and error payload, which is why every `*FetchError` event mirrors
// its `*Fetched` twin's identifying fields (see types.ts) — without it, an
// error meant for a *different* concurrent call could reject this one, same
// as an unmatched success could resolve it with someone else's result.
// Both listeners are torn down the moment either settles. This is the
// shared shape behind every "invoke a Tauri command, then wait for its
// correlated event" call in api.ts.
export function awaitCorrelatedEvent<TOk, TErr extends { message: string }>(
  trigger: () => Promise<unknown>,
  listenOk: (handler: (e: TOk) => void) => Promise<UnlistenFn>,
  listenErr: (handler: (e: TErr) => void) => Promise<UnlistenFn>,
  match?: (e: TOk | TErr) => boolean
): Promise<TOk> {
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
    listenErr((e) => {
      if (match && !match(e)) return;
      cleanup();
      reject(new Error(e.message));
    }).then((fn) => (unlistenErr = fn));
    trigger().catch((e) => {
      cleanup();
      reject(e);
    });
  });
}
