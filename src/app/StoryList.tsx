// Shared between Search and Bookmarks (locked in wayfinder ticket #5) —
// list/card rendering only differs by whether fandom shows per-row
// (Bookmarks spans fandoms; Search is already fandom-scoped) and whether
// dismiss is offered (not meaningful on an already-bookmarked row).
import { CloudCheck, Heart, WholeWord } from "lucide-react";
import { Chip, Icon } from "@lhak/style/react";
import { SOURCE_LABELS } from "$lib/filterOptions";
import { fmt } from "$lib/format";
import type { StoryRow } from "$lib/types";
import type { ViewMode } from "$lib/viewMode";
import { StatusBadges } from "./StatusBadges";
import { StoryActions, type StoryActionHandlers } from "./StoryActions";

export interface StoryListProps extends StoryActionHandlers {
  stories: StoryRow[];
  viewMode: ViewMode;
  showFandom?: boolean;
  includeDismiss?: boolean;
  onOpenStory: (s: StoryRow) => void;
  downloadingStoryId?: number | null;
  downloadProgress?: string;
  busy?: boolean;
}

// AO3 rows can carry a dozen+ freeform tags — capping the chip count
// (not just each chip's own width) is what actually keeps a card's
// height in check, since flex-wrap alone just keeps adding rows.
const MAX_TAG_CHIPS = 4;

function visibleTags(tags: string): { shown: string[]; extra: number } {
  const all = tags.split(", ");
  return { shown: all.slice(0, MAX_TAG_CHIPS), extra: Math.max(0, all.length - MAX_TAG_CHIPS) };
}

function isFullyDownloaded(story: StoryRow): boolean {
  return story.chapterCount > 0 && story.cachedChapterCount >= story.chapterCount;
}

/// The story's source as a category chip: AO3 takes the yellow hue (it was
/// gold), FFN stays plain. The word names it either way.
function SourceChip({ story }: { story: StoryRow }) {
  const label = SOURCE_LABELS[story.source];
  return story.source === "ao3" ? (
    <Chip hue="yellow" tint>
      {label}
    </Chip>
  ) : (
    <Chip>{label}</Chip>
  );
}

function OfflineChip() {
  return (
    <Chip title="All chapters downloaded — readable offline">
      <Icon icon={CloudCheck} /> Offline
    </Chip>
  );
}

export function StoryList({
  stories,
  viewMode,
  showFandom = false,
  includeDismiss = true,
  onOpenStory,
  downloadingStoryId = null,
  downloadProgress = "",
  busy = false,
  ...handlers
}: StoryListProps) {
  const actions = (story: StoryRow) => (
    <StoryActions
      story={story}
      includeDismiss={includeDismiss}
      downloadingStoryId={downloadingStoryId}
      downloadProgress={downloadProgress}
      busy={busy}
      fullyDownloaded={isFullyDownloaded(story)}
      {...handlers}
    />
  );
  const offline = (story: StoryRow) =>
    handlers.onDownloadAll !== undefined && isFullyDownloaded(story);
  const dim = (story: StoryRow) => (story.isRead || story.dismissed ? " fanf-dim" : "");

  if (viewMode === "list") {
    return (
      <ul className="fanf-list" aria-label="Stories">
        {stories.map((story) => (
          <li key={story.id} className={`fanf-list-row${dim(story)}`}>
            <div className="fanf-list-main">
              <button
                type="button"
                className="fanf-title"
                title={story.title}
                onClick={() => onOpenStory(story)}
              >
                {story.title}
              </button>
              <div className="fanf-meta">
                <span className="fanf-byline">
                  {showFandom ? `${story.author} · ${story.fandom}` : story.author}
                </span>
                <SourceChip story={story} />
                {offline(story) && <OfflineChip />}
                <StatusBadges status={story.status} ffnUpdatedAt={story.ffnUpdatedAt} />
              </div>
            </div>
            <div className="fanf-list-desc">
              <div className="fanf-summary-line" title={story.summary ?? undefined}>
                {story.summary ?? ""}
              </div>
              {story.tags && (
                <div className="fanf-tags-line" title={story.tags}>
                  {story.tags}
                </div>
              )}
            </div>
            <div className="fanf-stats">
              <span className="fanf-stat" title="Word count">
                <Icon icon={WholeWord} /> {fmt(story.wordCount)}
              </span>
              <span className="fanf-stat" title="Favorites">
                <Icon icon={Heart} /> {fmt(story.favoriteCount)}
              </span>
            </div>
            {actions(story)}
          </li>
        ))}
      </ul>
    );
  }

  return (
    <ul className="fanf-cards" aria-label="Stories">
      {stories.map((story) => {
        const tags = story.tags ? visibleTags(story.tags) : undefined;
        return (
          <li key={story.id} className={`fanf-card${dim(story)}`}>
            <div className="fanf-card-fandom-row">
              <div className="fanf-card-fandom" title={story.fandom}>
                {story.fandom}
              </div>
              {offline(story) && <OfflineChip />}
              <SourceChip story={story} />
            </div>
            <h3 className="fanf-card-title">
              <button type="button" className="fanf-title" onClick={() => onOpenStory(story)}>
                {story.title}
              </button>
            </h3>
            <p className="fanf-card-summary" title={story.summary ?? undefined}>
              {story.summary ?? ""}
            </p>
            {tags && (
              <div className="fanf-chip-row">
                {tags.shown.map((t, i) => (
                  // A story can repeat a tag, so the index keeps keys unique.
                  <Chip key={`${i}:${t}`} title={t} className="fanf-tag">
                    {t}
                  </Chip>
                ))}
                {tags.extra > 0 && (
                  <Chip title={story.tags ?? undefined} className="fanf-tag">
                    +{tags.extra} more
                  </Chip>
                )}
              </div>
            )}
            <div className="fanf-chip-row">
              <Chip>{story.rating}</Chip>
              <Chip title="Word count">
                <Icon icon={WholeWord} /> {fmt(story.wordCount)}
              </Chip>
              <Chip title="Favorites">
                <Icon icon={Heart} /> {fmt(story.favoriteCount)}
              </Chip>
            </div>
            <div className="fanf-card-foot">
              <div className="fanf-card-status">
                <StatusBadges status={story.status} ffnUpdatedAt={story.ffnUpdatedAt} />
              </div>
              {actions(story)}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
