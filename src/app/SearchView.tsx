import {
  Button,
  Checkbox,
  EmptyState,
  SearchBar,
  SegmentedControl,
  Select,
} from "@lhak/style/react";
import { SORT_ITEMS, STATUS_LABELS, WORD_COUNT_PRESETS } from "$lib/filterOptions";
import type { SearchScreen } from "$lib/searchScreen";
import type { SortBy, StoryRow, StorySource, StoryStatus } from "$lib/types";
import type { ViewMode } from "$lib/viewMode";
import { FandomPicker } from "./FandomPicker";
import { SearchDialog } from "./SearchDialog";
import { StoryList } from "./StoryList";
import type { StoryActionHandlers } from "./StoryActions";
import { useStore } from "./useStore";
import { ViewToggle } from "./ViewToggle";

// A segmented control's values are strings: "any" stands for no filter.
const ANY = "any";

const SOURCE_ITEMS = [
  { value: ANY, label: "All" },
  { value: "ffn", label: "FFN" },
  { value: "ao3", label: "AO3" },
];

const LENGTH_ITEMS = [
  { value: ANY, label: "Any length" },
  ...WORD_COUNT_PRESETS.map((p) => ({ value: String(p.value), label: p.label })),
];

const STATUS_ITEMS = [
  { value: ANY, label: "Any status" },
  ...Object.entries(STATUS_LABELS).map(([value, label]) => ({ value, label })),
];

export interface SearchViewProps extends StoryActionHandlers {
  search: SearchScreen;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  onOpenStory: (s: StoryRow) => void;
  /** "By URL" in the Search dialog bookmarked a story. */
  onStoryAdded: () => void;
}

function LoadMore({ search }: { search: SearchScreen }) {
  return (
    <div className="fanf-load-more">
      <Button onClick={() => void search.loadMore()} disabled={search.loadingMore}>
        {search.loadingMore ? "Loading…" : "Load more"}
      </Button>
      {search.loadMoreError && <span className="fanf-error-text">{search.loadMoreError}</span>}
    </div>
  );
}

export function SearchView({
  search: searchStore,
  viewMode,
  onViewModeChange,
  onOpenStory,
  onStoryAdded,
  ...handlers
}: SearchViewProps) {
  const search = useStore(searchStore);
  const openDialog = () => (search.modalOpen = true);
  return (
    <>
      <SearchDialog
        open={search.modalOpen}
        onClose={() => (search.modalOpen = false)}
        onImported={(fandom, source) => void search.handleImported(fandom, source)}
        onStoryAdded={() => {
          search.modalOpen = false;
          onStoryAdded();
        }}
      />
      <div className="fanf-filterbar">
        <div className="fanf-filterbar-row">
          <FandomPicker
            fandoms={search.fandoms}
            selected={search.fandom}
            onSelect={(f) => (search.fandom = f)}
            pinnedFandoms={search.pinnedFandoms}
            onPinnedChange={(pinned) => (search.pinnedFandoms = pinned)}
            onRename={(oldFandom, newFandom) => void search.renameFandom(oldFandom, newFandom)}
          />
          <Button variant="primary" onClick={openDialog}>
            Search…
          </Button>
          <SearchBar
            label="Search stories"
            placeholder="Search title, author, or summary…"
            value={search.text}
            onValueChange={(text) => (search.text = text)}
            className="fanf-search"
          />
          <Select<SortBy>
            aria-label="Sort"
            items={SORT_ITEMS}
            value={search.sortBy}
            onValueChange={(v) => {
              if (v !== null) search.sortBy = v;
            }}
          />
          <ViewToggle value={viewMode} onChange={onViewModeChange} />
        </div>
        <div className="fanf-filterbar-row">
          <SegmentedControl
            aria-label="Source"
            items={SOURCE_ITEMS}
            value={search.sourceFilter ?? ANY}
            onValueChange={(v) =>
              (search.sourceFilter = v === ANY ? undefined : (v as StorySource))
            }
          />
          <SegmentedControl
            aria-label="Minimum word count"
            items={LENGTH_ITEMS}
            value={search.minWordCount === undefined ? ANY : String(search.minWordCount)}
            onValueChange={(v) => (search.minWordCount = v === ANY ? undefined : Number(v))}
          />
          <SegmentedControl
            aria-label="Status"
            items={STATUS_ITEMS}
            value={search.status ?? ANY}
            onValueChange={(v) => (search.status = v === ANY ? undefined : (v as StoryStatus))}
          />
          <Checkbox
            checked={search.hideRead}
            onCheckedChange={(checked) => (search.hideRead = checked)}
          >
            Hide read
          </Checkbox>
          <Checkbox
            checked={search.hideDismissed}
            onCheckedChange={(checked) => (search.hideDismissed = checked)}
          >
            Hide dismissed
          </Checkbox>
          <Checkbox
            checked={search.hideBookmarked}
            onCheckedChange={(checked) => (search.hideBookmarked = checked)}
          >
            Hide bookmarked
          </Checkbox>
        </div>
      </div>

      {search.stories.length === 0 ? (
        search.fandom ? (
          <EmptyState
            title={
              search.fandoms.includes(search.fandom)
                ? `You've gone through everything imported for "${search.fandom}" with these filters.`
                : `No stories for "${search.fandom}" yet.`
            }
            action={<LoadMore search={search} />}
          />
        ) : (
          <EmptyState
            title="No stories yet."
            description="Search FanFiction.net or AO3 to pull some in."
            action={<Button onClick={openDialog}>Search…</Button>}
          />
        )
      ) : (
        <>
          <StoryList
            stories={search.stories}
            viewMode={viewMode}
            onOpenStory={onOpenStory}
            {...handlers}
          />
          {search.fandom && <LoadMore search={search} />}
        </>
      )}
    </>
  );
}
