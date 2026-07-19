export interface SearchFilterPrefs {
  hideRead: boolean;
  hideDismissed: boolean;
  hideBookmarked: boolean;
}

const SEARCH_KEY = "ffn-search-filters";
const SEARCH_DEFAULTS: SearchFilterPrefs = { hideRead: true, hideDismissed: true, hideBookmarked: false };

export function getSearchFilterPrefs(): SearchFilterPrefs {
  if (typeof localStorage === "undefined") return SEARCH_DEFAULTS;
  try {
    const raw = localStorage.getItem(SEARCH_KEY);
    if (!raw) return SEARCH_DEFAULTS;
    return { ...SEARCH_DEFAULTS, ...JSON.parse(raw) };
  } catch {
    return SEARCH_DEFAULTS;
  }
}

export function saveSearchFilterPrefs(prefs: SearchFilterPrefs) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(SEARCH_KEY, JSON.stringify(prefs));
}

const BOOKMARKS_HIDE_READ_KEY = "ffn-bookmarks-hide-read";

export function getBookmarksHideRead(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(BOOKMARKS_HIDE_READ_KEY) === "true";
}

export function saveBookmarksHideRead(value: boolean) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(BOOKMARKS_HIDE_READ_KEY, String(value));
}
