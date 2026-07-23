import { definePersistedPref } from "./persistedPref";

export interface SearchFilterPrefs {
  hideRead: boolean;
  hideDismissed: boolean;
  hideBookmarked: boolean;
}

const SEARCH_DEFAULTS: SearchFilterPrefs = { hideRead: true, hideDismissed: true, hideBookmarked: false };

// Merges over SEARCH_DEFAULTS on every read (not just when nothing's
// stored) — a blob saved by an older app version missing a newer field
// still gets that field's default, rather than `undefined`.
const searchPref = definePersistedPref<SearchFilterPrefs>("ffn-search-filters", SEARCH_DEFAULTS, {
  deserialize: (raw) => ({ ...SEARCH_DEFAULTS, ...JSON.parse(raw) })
});

export const getSearchFilterPrefs = searchPref.get;
export const saveSearchFilterPrefs = searchPref.save;

const bookmarksHideReadPref = definePersistedPref<boolean>("ffn-bookmarks-hide-read", false);

export const getBookmarksHideRead = bookmarksHideReadPref.get;
export const saveBookmarksHideRead = bookmarksHideReadPref.save;
