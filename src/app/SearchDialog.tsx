// Discovery entry point for both sources (SPEC.md §6/§8): fetches page 1 of a
// fandom/tag listing, server-side sorted and filtered, so what lands first is
// already the most relevant match. The two sources' fandom inputs differ (FFN
// has a cached category + fandom index; AO3 has neither, robots.txt blocks its
// /autocomplete/ endpoint); that is deliberately the only thing that differs.
// Word-count/status filters and the submit flow are shared.
import { useEffect, useState } from "react";
import {
  Button,
  Combobox,
  Dialog,
  DialogPopup,
  SegmentedControl,
  Select,
  SettingRow,
  TextField,
} from "@lhak/style/react";
import { STATUS_LABELS, WORD_COUNT_PRESETS } from "$lib/filterOptions";
import { ALL, SearchModalScreen, type SearchMode } from "$lib/searchModalScreen";
import { FFN_CATEGORIES, type StorySource, type StoryStatus } from "$lib/types";
import { useStore } from "./useStore";

// A segmented control's values are strings: "any" stands for no filter.
const ANY = "any";

const MODE_ITEMS = [
  { value: "ffn", label: "FanFiction.net" },
  { value: "ao3", label: "Archive of Our Own" },
  { value: "url", label: "By URL" },
] as const;

const CATEGORY_ITEMS = [
  { value: ALL, label: "All categories" },
  ...FFN_CATEGORIES.map((c) => ({ value: c.value as string, label: c.label as string })),
];

const LENGTH_ITEMS = [
  { value: ANY, label: "Any" },
  ...WORD_COUNT_PRESETS.map((p) => ({ value: String(p.value), label: p.label })),
];

/** The fandom list shows the first 50 matches. */
const FANDOM_LIMIT = 50;

const fandomKey = (f: { category: string; fandom: string }) => `${f.category}:${f.fandom}`;

export interface SearchDialogProps {
  open: boolean;
  onClose: () => void;
  onImported: (fandom: string, source: StorySource) => void;
  /** "By URL" bookmarks the story it adds, then calls this (not `onImported`). */
  onStoryAdded: () => void;
}

export function SearchDialog({ open, onClose, ...callbacks }: SearchDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogPopup title="Search">
        <SearchForm {...callbacks} />
      </DialogPopup>
    </Dialog>
  );
}

/// The popup's body: mounted per open, so each open starts a fresh Screen.
function SearchForm({ onImported, onStoryAdded }: Omit<SearchDialogProps, "open" | "onClose">) {
  const [store] = useState(() => new SearchModalScreen(onImported, onStoryAdded));
  const screen = useStore(store);
  useEffect(() => {
    store.start();
    return () => store.dispose();
  }, [store]);

  // The Screen filters (accent-folded, `filteredOptions`); the combobox only
  // shows. The label is the bare fandom name: the input shows it once chosen,
  // and it becomes `fandomQuery` again.
  const fandomItems = screen.filteredOptions.slice(0, FANDOM_LIMIT).map((f) => ({
    value: fandomKey(f),
    label: f.fandom,
  }));
  const byKey = new Map(screen.fandomOptions.map((f) => [fandomKey(f), f]));
  const statusItems = [
    { value: ANY, label: "Any" },
    ...screen.statusOptions.map((s) => ({ value: s, label: STATUS_LABELS[s] })),
  ];
  const indexLabel = screen.indexing
    ? screen.indexProgress || "Fetching…"
    : screen.fandomOptions.length === 0
      ? screen.category === ALL
        ? "Index all categories (one-time, ~9 fetches)"
        : "Fetch fandom list for this category"
      : screen.category === ALL
        ? "Re-index all categories"
        : "Refresh this category";
  const submitLabel = screen.searching
    ? screen.mode === "url"
      ? "Adding…"
      : "Searching…"
    : screen.mode === "url"
      ? "Add story"
      : "Search";

  return (
    <form
      className="fanf-search-form"
      onSubmit={(e) => {
        e.preventDefault();
        void screen.runSearch();
      }}
    >
      <SegmentedControl<SearchMode>
        aria-label="Source"
        items={MODE_ITEMS}
        value={screen.mode}
        onValueChange={(mode) => (screen.mode = mode)}
      />

      {screen.mode === "ffn" && (
        <>
          <Select<string>
            label="Category"
            items={CATEGORY_ITEMS}
            value={screen.category}
            onValueChange={(v) => {
              if (v !== null) screen.category = v;
            }}
          />
          {screen.fandomOptions.length > 0 && (
            <Combobox<string>
              label="Fandom"
              placeholder="Type to search…"
              items={fandomItems}
              filter={null}
              inputValue={screen.fandomQuery}
              onInputValueChange={(query) => (screen.fandomQuery = query)}
              empty="No matches"
              value={screen.selectedFandom ? fandomKey(screen.selectedFandom) : null}
              onValueChange={(key) => {
                const entry = key === null ? undefined : byKey.get(key);
                if (entry) screen.selectFandom(entry);
                else screen.selectedFandom = undefined;
              }}
            />
          )}
          <div className="fanf-search-index">
            <Button
              variant={screen.fandomOptions.length === 0 ? "secondary" : "ghost"}
              onClick={() => void screen.refreshIndex()}
              disabled={screen.indexing}
            >
              {indexLabel}
            </Button>
            {screen.indexError && <p className="fanf-error-text">{screen.indexError}</p>}
          </div>
        </>
      )}

      {screen.mode === "ao3" && (
        <>
          <SettingRow label="Fandom">
            <TextField
              placeholder="e.g. Naruto"
              value={screen.ao3FandomName}
              onChange={(e) => (screen.ao3FandomName = e.currentTarget.value)}
            />
          </SettingRow>
          <SettingRow
            label="AO3 tag"
            help={
              'AO3\'s exact tag string, not a search term: the same as pasting it into AO3\'s own search ("Harry Potter" is tagged "Harry Potter - J.K. Rowling"). Saved after a successful search, so you type it once per fandom.'
            }
          >
            <TextField
              placeholder="e.g. Naruto (Anime & Manga)"
              value={screen.ao3Tag}
              onChange={(e) => (screen.ao3Tag = e.currentTarget.value)}
            />
          </SettingRow>
        </>
      )}

      {screen.mode === "url" && (
        <SettingRow
          label="Story URL"
          help="A FanFiction.net story or chapter link, or an AO3 work link. Adds that one story, with its full chapter list, to your Bookmarks."
        >
          <TextField
            placeholder="https://www.fanfiction.net/s/14583034/1/…"
            value={screen.url}
            onChange={(e) => (screen.url = e.currentTarget.value)}
          />
        </SettingRow>
      )}

      {screen.mode !== "url" && (
        <>
          <SettingRow label="Minimum length" group>
            <SegmentedControl
              aria-label="Minimum length"
              items={LENGTH_ITEMS}
              value={screen.minWordCount === undefined ? ANY : String(screen.minWordCount)}
              onValueChange={(v) => (screen.minWordCount = v === ANY ? undefined : Number(v))}
            />
          </SettingRow>
          <SettingRow label="Status" group>
            <SegmentedControl
              aria-label="Status"
              items={statusItems}
              value={screen.status ?? ANY}
              onValueChange={(v) => (screen.status = v === ANY ? undefined : (v as StoryStatus))}
            />
          </SettingRow>
          <p className="fanf-search-note">
            {screen.mode === "ffn"
              ? "Pulls the first page, sorted by favorites: FFN's own ranking, not ours."
              : "Pulls the first page, sorted by bookmarks: AO3's closest analog to FFN's favorites."}{" "}
            Use Load more afterward for deeper results.
          </p>
        </>
      )}

      {screen.error && (
        <p className="fanf-error-text" role="alert">
          {screen.error}
        </p>
      )}

      <Button
        type="submit"
        variant="primary"
        className="fanf-search-submit"
        disabled={screen.searching || !screen.canSubmit}
      >
        {submitLabel}
      </Button>
    </form>
  );
}
