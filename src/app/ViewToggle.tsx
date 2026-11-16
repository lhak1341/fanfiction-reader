import { LayoutGrid, List } from "lucide-react";
import { Icon, SegmentedControl } from "@lhak/style/react";
import type { ViewMode } from "$lib/viewMode";

const ITEMS = [
  { value: "list", label: <Icon icon={List} label="List view" /> },
  { value: "card", label: <Icon icon={LayoutGrid} label="Card view" /> },
] as const;

/// List or cards: one persisted preference shared by Search and Bookmarks.
export function ViewToggle({
  value,
  onChange,
}: {
  value: ViewMode;
  onChange: (mode: ViewMode) => void;
}) {
  return (
    <SegmentedControl<ViewMode>
      aria-label="View"
      items={ITEMS}
      value={value}
      onValueChange={(v) => onChange(v)}
    />
  );
}
