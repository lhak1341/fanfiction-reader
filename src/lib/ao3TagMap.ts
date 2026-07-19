// Maps a local fandom name to the AO3 tag string that actually finds it
// (SPEC.md §8) — AO3 tags don't slugify predictably from a fandom name
// (e.g. "Harry Potter" is the tag "Harry Potter - J.K. Rowling"), and
// robots.txt disallows AO3's /autocomplete/ endpoint, so there's no way to
// look this up automatically. The user maps each fandom once (typed into
// the search modal); it's reused automatically after that. Same
// localStorage-preference pattern as pinnedFandoms.ts/readerPrefs.ts —
// nothing here is fetched data, so it doesn't belong in the DB.

const STORAGE_KEY = "ao3-tag-map";

function readMap(): Record<string, string> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

export function getAo3Tag(fandom: string): string | undefined {
  return readMap()[fandom];
}

export function saveAo3Tag(fandom: string, tag: string) {
  if (typeof localStorage === "undefined") return;
  const map = readMap();
  map[fandom] = tag;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
}
