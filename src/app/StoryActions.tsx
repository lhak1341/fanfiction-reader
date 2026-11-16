import { Check, Download, Star, X } from "lucide-react";
import { RowAction, RowActions } from "@lhak/style/react";
import type { StoryRow } from "$lib/types";

export interface StoryActionHandlers {
  onToggleRead: (s: StoryRow) => void;
  onToggleDismissed: (s: StoryRow) => void;
  onToggleBookmarked: (s: StoryRow) => void;
  /// Bookmarks-only "download all chapters" action — omitted on Search.
  onDownloadAll?: (s: StoryRow) => void;
}

export interface StoryActionsProps extends StoryActionHandlers {
  story: StoryRow;
  includeDismiss: boolean;
  downloadingStoryId: number | null;
  downloadProgress: string;
  /// A bulk job (refresh or any download) is running — Download stays disabled.
  busy: boolean;
  fullyDownloaded: boolean;
}

/// A story's flag toggles. Each shows its state as pressed, so the row stays
/// readable without hovering (the actions are always shown, not revealed).
export function StoryActions({
  story,
  includeDismiss,
  onToggleRead,
  onToggleDismissed,
  onToggleBookmarked,
  onDownloadAll,
  downloadingStoryId,
  downloadProgress,
  busy,
  fullyDownloaded,
}: StoryActionsProps) {
  const downloading = downloadingStoryId === story.id;
  return (
    <RowActions label={`Actions for ${story.title}`} className="fanf-story-actions">
      <RowAction
        icon={Check}
        label="Mark as read"
        aria-pressed={story.isRead}
        onClick={() => onToggleRead(story)}
      />
      {includeDismiss && (
        <RowAction
          icon={X}
          label="Dismiss — not for me"
          aria-pressed={story.dismissed}
          onClick={() => onToggleDismissed(story)}
        />
      )}
      <RowAction
        icon={Star}
        label="Bookmark"
        aria-pressed={story.bookmarked}
        onClick={() => onToggleBookmarked(story)}
      />
      {onDownloadAll && (downloading || !fullyDownloaded) && (
        <RowAction
          icon={Download}
          label={
            downloading
              ? downloadProgress || "Downloading…"
              : busy
                ? "Wait for the current refresh/download to finish"
                : "Download all chapters for offline reading"
          }
          className={downloading ? "fanf-spinning" : undefined}
          disabled={busy}
          onClick={() => onDownloadAll(story)}
        />
      )}
    </RowActions>
  );
}
