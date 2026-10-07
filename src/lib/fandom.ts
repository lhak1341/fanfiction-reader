// Fandom-identity helpers that don't belong to any one component (see
// CONTEXT.md's `Fandom` entry). Two things live here:
//
// - The AO3 fandom-name -> tag mapping: AO3 tags don't slugify predictably
//   from a fandom name (e.g. "Harry Potter" is the tag "Harry Potter -
//   J.K. Rowling"), and robots.txt disallows AO3's /autocomplete/ endpoint,
//   so there's no way to look this up automatically. The user maps each
//   fandom once (typed into the search modal); it's reused automatically
//   after that. Same localStorage-preference pattern as
//   pinnedFandoms.ts/readerPrefs.ts — nothing here is fetched data, so it
//   doesn't belong in the DB.
//
// - Renaming/merging a fandom bucket needs every other piece of client
//   state that names the old fandom to follow (a pin, the current search
//   selection, the AO3 tag map) or the rename silently orphans them.

import { definePersistedPref } from "./persistedPref";

const tagMapPref = definePersistedPref<Record<string, string>>("ao3-tag-map", {});

export function getAo3Tag(fandom: string): string | undefined {
  return tagMapPref.get()[fandom];
}

export function saveAo3Tag(fandom: string, tag: string) {
  tagMapPref.save({ ...tagMapPref.get(), [fandom]: tag });
}

/// Moves the AO3 tag saved under `oldFandom` to `newFandom` so a rename doesn't
/// make the user retype it. If `newFandom` already has a tag (a merge into an
/// existing fandom) that one wins — it's the one already in use.
export function renameAo3Tag(oldFandom: string, newFandom: string) {
  const map = tagMapPref.get();
  if (!(oldFandom in map)) return;
  const { [oldFandom]: tag, ...rest } = map;
  tagMapPref.save({ ...rest, [newFandom]: map[newFandom] ?? tag });
}

/// Carries a fandom rename/merge across the client state that names the old
/// fandom — a pinned fandom moves to the new name rather than disappearing;
/// the current search selection follows rather than reverting to "All
/// fandoms". Pure — the caller still owns calling `api.renameFandom` and
/// applying the returned state.
export function applyFandomRename(
  oldFandom: string,
  newFandom: string,
  pinnedFandoms: Set<string>,
  currentSearchFandom: string | undefined
): { pinnedFandoms: Set<string>; searchFandom: string | undefined } {
  let pinned = pinnedFandoms;
  if (pinned.has(oldFandom)) {
    pinned = new Set(pinned);
    pinned.delete(oldFandom);
    pinned.add(newFandom);
  }
  const searchFandom = currentSearchFandom === oldFandom ? newFandom : currentSearchFandom;
  return { pinnedFandoms: pinned, searchFandom };
}
