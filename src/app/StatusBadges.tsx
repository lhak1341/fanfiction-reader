import { Calendar } from "lucide-react";
import { Chip, Icon } from "@lhak/style/react";
import { STATUS_LABELS } from "$lib/filterOptions";
import { shortDate } from "$lib/format";
import type { StoryStatus } from "$lib/types";

/// A story's status as a kit status chip: its icon and its word say it, never
/// the colour alone. An ongoing story also shows when it last updated.
export function StatusBadges({
  status,
  ffnUpdatedAt,
}: {
  status: StoryStatus;
  ffnUpdatedAt: string | null;
}) {
  const updated = status === "in_progress" && ffnUpdatedAt ? shortDate(ffnUpdatedAt) : "";
  return (
    <>
      {status === "complete" ? (
        <Chip status="success">{STATUS_LABELS.complete}</Chip>
      ) : (
        <Chip status="info">{STATUS_LABELS.in_progress}</Chip>
      )}
      {updated && (
        <span className="fanf-updated" title={`Last updated ${ffnUpdatedAt}`}>
          <Icon icon={Calendar} /> {updated}
        </span>
      )}
    </>
  );
}
