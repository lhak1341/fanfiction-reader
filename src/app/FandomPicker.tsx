// Shared between Search (pinnable, full fandom list) and Bookmarks
// (not pinnable, scoped to fandoms actually present among bookmarks).
// Locked in wayfinder ticket #5.
import { useState, type KeyboardEvent } from "react";
import { Check, ChevronDown, Pencil, Star, X } from "lucide-react";
import { Button, Icon, Popover, PopoverPopup, PopoverTrigger, TextField } from "@lhak/style/react";
import { fold } from "$lib/normalize";

export interface FandomPickerProps {
  fandoms: string[];
  selected: string | undefined;
  onSelect: (f: string | undefined) => void;
  /// Given, the list shows pins and a Pinned section first.
  pinnedFandoms?: Set<string>;
  onPinnedChange?: (pinned: Set<string>) => void;
  /// Renames/merges a fandom bucket (updates every story that has it) —
  /// cleans up duplicate-looking entries (e.g. an AO3 import that kept
  /// AO3's own canonical tag text instead of matching the FFN side's
  /// name) or messy AO3 crossover buckets (a comma-joined multi-fandom
  /// string) by merging them into an existing, cleaner fandom name.
  /// Omit to hide the rename control entirely (e.g. Bookmarks' picker).
  onRename?: (oldFandom: string, newFandom: string) => void;
}

export function FandomPicker({
  fandoms,
  selected,
  onSelect,
  pinnedFandoms,
  onPinnedChange,
  onRename,
}: FandomPickerProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [renaming, setRenaming] = useState<string | undefined>(undefined);
  const [renameValue, setRenameValue] = useState("");

  const pinnable = pinnedFandoms !== undefined && onPinnedChange !== undefined;
  const visible = fandoms.filter((f) => fold(f).includes(fold(search)));
  const pinnedVisible = pinnable ? visible.filter((f) => pinnedFandoms.has(f)) : [];
  const restVisible = pinnable ? visible.filter((f) => !pinnedFandoms.has(f)) : visible;

  function select(f: string | undefined) {
    onSelect(f);
    setOpen(false);
  }

  function togglePin(f: string) {
    if (!pinnable) return;
    const next = new Set(pinnedFandoms);
    if (next.has(f)) next.delete(f);
    else next.add(f);
    onPinnedChange(next);
  }

  function submitRename(f: string) {
    const next = renameValue.trim();
    setRenaming(undefined);
    if (!next || next === f) return;
    onRename?.(f, next);
  }

  function onRenameKey(f: string, e: KeyboardEvent) {
    if (e.key === "Enter") submitRename(f);
    if (e.key === "Escape") {
      // Escape ends the rename, not the popover.
      e.stopPropagation();
      setRenaming(undefined);
    }
  }

  const row = (f: string, pinned: boolean) => (
    <li key={f} className="fanf-fandom-row">
      {renaming === f ? (
        <>
          <TextField
            aria-label={`New name for ${f}`}
            className="fanf-fandom-rename"
            value={renameValue}
            onChange={(e) => setRenameValue(e.currentTarget.value)}
            onKeyDown={(e) => onRenameKey(f, e)}
            autoFocus
          />
          <Button variant="ghost" iconOnly aria-label="Save" onClick={() => submitRename(f)}>
            <Icon icon={Check} />
          </Button>
          <Button
            variant="ghost"
            iconOnly
            aria-label="Cancel"
            onClick={() => setRenaming(undefined)}
          >
            <Icon icon={X} />
          </Button>
        </>
      ) : (
        <>
          {pinnable && (
            <Button
              variant="ghost"
              iconOnly
              aria-label={pinned ? `Unpin ${f}` : `Pin ${f}`}
              aria-pressed={pinned}
              className="fanf-pin"
              onClick={() => togglePin(f)}
            >
              <Icon icon={Star} />
            </Button>
          )}
          <button
            type="button"
            className="fanf-fandom-option"
            aria-current={selected === f ? "true" : undefined}
            onClick={() => select(f)}
          >
            {f}
          </button>
          {onRename && (
            <Button
              variant="ghost"
              iconOnly
              aria-label={`Rename or merge ${f}`}
              onClick={() => {
                setRenaming(f);
                setRenameValue(f);
              }}
            >
              <Icon icon={Pencil} />
            </Button>
          )}
        </>
      )}
    </li>
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger render={<Button className="fanf-fandom-trigger" />}>
        Fandom: {selected ?? "All fandoms"}
        <Icon icon={ChevronDown} />
      </PopoverTrigger>
      <PopoverPopup aria-label="Fandoms" align="start" className="fanf-fandom-popup">
        <TextField
          aria-label="Search fandoms"
          placeholder="Search fandoms…"
          value={search}
          onChange={(e) => setSearch(e.currentTarget.value)}
        />
        <ul className="fanf-fandom-options">
          <li className="fanf-fandom-row">
            <button
              type="button"
              className="fanf-fandom-option"
              aria-current={selected === undefined ? "true" : undefined}
              onClick={() => select(undefined)}
            >
              All fandoms
            </button>
          </li>
          {pinnedVisible.length > 0 && (
            <>
              <li className="fanf-fandom-section" aria-hidden="true">
                Pinned
              </li>
              {pinnedVisible.map((f) => row(f, true))}
              <li className="fanf-fandom-section" aria-hidden="true">
                All fandoms
              </li>
            </>
          )}
          {restVisible.map((f) => row(f, false))}
          {visible.length === 0 && <li className="fanf-empty">No fandoms match "{search}"</li>}
        </ul>
      </PopoverPopup>
    </Popover>
  );
}
