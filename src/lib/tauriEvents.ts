import type { UnlistenFn } from "@tauri-apps/api/event";

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
