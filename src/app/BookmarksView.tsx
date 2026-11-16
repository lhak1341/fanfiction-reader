import { Checkbox, EmptyState, Select } from "@lhak/style/react";
import type { BookmarksScreen } from "$lib/bookmarksScreen";
import { SORT_ITEMS } from "$lib/filterOptions";
import type { SortBy, StoryRow } from "$lib/types";
import type { ViewMode } from "$lib/viewMode";
import { FandomPicker } from "./FandomPicker";
import { StoryList } from "./StoryList";
import type { StoryActionHandlers } from "./StoryActions";
import { useStore } from "./useStore";
import { ViewToggle } from "./ViewToggle";

export interface BookmarksViewProps extends StoryActionHandlers {
  bookmarks: BookmarksScreen;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  onOpenStory: (s: StoryRow) => void;
}

function fmtChapter(story: StoryRow) {
  return story.lastChapterNum
    ? `Ch. ${story.lastChapterNum} of ${story.chapterCount}`
    : "Not started";
}

export function BookmarksView({
  bookmarks: bookmarksStore,
  viewMode,
  onViewModeChange,
  onOpenStory,
  ...handlers
}: BookmarksViewProps) {
  const bookmarks = useStore(bookmarksStore);
  return (
    <>
      {bookmarks.continueReading.length > 0 && (
        <section className="fanf-continue" aria-labelledby="fanf-continue-title">
          <h2 id="fanf-continue-title">Continue Reading</h2>
          <div className="fanf-continue-row">
            {bookmarks.continueReading.map((story) => (
              <button
                key={story.id}
                type="button"
                className="fanf-continue-card"
                onClick={() => onOpenStory(story)}
              >
                <span className="fanf-continue-title">{story.title}</span>
                <span className="fanf-continue-meta">
                  {story.fandom} · {fmtChapter(story)}
                </span>
              </button>
            ))}
          </div>
        </section>
      )}

      <div className="fanf-filterbar">
        <div className="fanf-filterbar-row">
          <FandomPicker
            fandoms={bookmarks.fandoms}
            selected={bookmarks.fandom}
            onSelect={(f) => (bookmarks.fandom = f)}
          />
          <Select<SortBy>
            aria-label="Sort"
            items={SORT_ITEMS}
            value={bookmarks.sortBy}
            onValueChange={(v) => {
              if (v !== null) bookmarks.sortBy = v;
            }}
          />
          <Checkbox
            checked={bookmarks.hideRead}
            onCheckedChange={(checked) => (bookmarks.hideRead = checked)}
          >
            Hide read
          </Checkbox>
          <ViewToggle value={viewMode} onChange={onViewModeChange} />
        </div>
      </div>

      {bookmarks.stories.length === 0 ? (
        <EmptyState
          title="No bookmarks yet."
          description="Bookmark a story from Search to see it here."
        />
      ) : (
        <StoryList
          stories={bookmarks.stories}
          viewMode={viewMode}
          showFandom
          includeDismiss={false}
          onOpenStory={onOpenStory}
          downloadingStoryId={bookmarks.job?.kind === "download" ? bookmarks.job.storyId : null}
          downloadProgress={bookmarks.progress.label}
          busy={bookmarks.busy}
          {...handlers}
        />
      )}
    </>
  );
}
