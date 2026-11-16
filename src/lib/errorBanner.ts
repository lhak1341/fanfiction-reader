// The page's failure banner (architecture review candidate, 2026-10-08).
// It used to be one shared string every loader overwrote, and loaders cleared
// it on success — so a Refresh / Download-all failure was wiped by the next
// `search.loadStories()` the page ran. Now each failing *operation* owns one
// message under its own key: only that operation succeeding again (or the
// user dismissing it) removes it. A plain `Store` class so tests can build
// their own instance; `src/app/App.tsx` creates the one the screens share.

import { Store } from "./store";

/// One key per operation, not per screen: a routine Bookmarks reload must not
/// clear a Refresh failure the user hasn't read yet.
export type BannerKey =
  | "search.fandoms"
  | "search.stories"
  | "search.rename"
  | "bookmarks.fandoms"
  | "bookmarks.list"
  | "bookmarks.continue"
  | "bookmarks.refresh"
  | "bookmarks.download"
  | "flag";

export interface BannerMessage {
  key: BannerKey;
  text: string;
}

/// `success`: the message clears when the operation succeeds (loads).
/// `start`: it clears when the operation begins and is never cleared on
/// success, so the operation can `report()` a partial failure of its own (bulk
/// jobs, whose failures don't throw).
export type ClearOn = "start" | "success";

function describe(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export class ErrorBanner extends Store {
  /// First-seen order; a repeat on the same key replaces its text in place.
  messages: BannerMessage[] = [];

  /// Runs `op`, reporting a throw as `"<prefix>: <error>"` under `key`.
  /// Returns whether it succeeded.
  async run(
    key: BannerKey,
    prefix: string,
    op: () => Promise<unknown>,
    opts: { clearOn?: ClearOn } = {}
  ): Promise<boolean> {
    const clearOn = opts.clearOn ?? "success";
    if (clearOn === "start") this.dismiss(key);
    try {
      await op();
    } catch (e) {
      this.report(key, `${prefix}: ${describe(e)}`);
      return false;
    }
    if (clearOn === "success") this.dismiss(key);
    return true;
  }

  report(key: BannerKey, text: string) {
    if (this.messages.some((m) => m.key === key)) {
      this.messages = this.messages.map((m) => (m.key === key ? { key, text } : m));
    } else {
      this.messages = [...this.messages, { key, text }];
    }
  }

  dismiss(key: BannerKey) {
    if (this.messages.some((m) => m.key === key)) {
      this.messages = this.messages.filter((m) => m.key !== key);
    }
  }
}
