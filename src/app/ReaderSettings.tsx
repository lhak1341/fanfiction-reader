import { Type } from "lucide-react";
import {
  Button,
  Icon,
  NumberStepper,
  Popover,
  PopoverPopup,
  PopoverTrigger,
  SegmentedControl,
  SettingGroup,
  SettingRow,
  TextField,
} from "@lhak/style/react";
import {
  readerTypography,
  resetReaderType,
  type ReaderPrefs,
  type ReadingFace,
} from "$lib/readerPrefs";

/** The prefs a stepper sets. */
type NumericPref = {
  [K in keyof ReaderPrefs]: ReaderPrefs[K] extends number | null ? K : never;
}[keyof ReaderPrefs];

const FACE_ITEMS = [
  { value: "serif", label: "Serif" },
  { value: "sans", label: "Sans" },
  { value: "custom", label: "Custom" },
] as const;

const ON_OFF = [
  { value: "on", label: "On" },
  { value: "off", label: "Off" },
] as const;

/// The reading font panel. Serif and Sans are Bookerly and Work Sans at their
/// own weight and tracking, with a reset for the shared settings; Custom is a
/// typed font at the reader's own weight and tracking (Bookerly's until set).
export function ReaderSettings({
  prefs,
  onChange,
}: {
  prefs: ReaderPrefs;
  onChange: (patch: Partial<ReaderPrefs>) => void;
}) {
  const t = readerTypography(prefs);
  // Base UI reports `null` for a cleared field: keep the last value then.
  // Rounded so 1.45 + 0.1 saves 1.55, not a float tail.
  const set = (key: NumericPref) => (value: number | null) => {
    if (value !== null) onChange({ [key]: Math.round(value * 1000) / 1000 });
  };
  return (
    <Popover>
      <PopoverTrigger render={<Button variant="ghost" iconOnly aria-label="Reading settings" />}>
        <Icon icon={Type} />
      </PopoverTrigger>
      <PopoverPopup title="Reading settings" className="fanf-reader-settings">
        <SettingGroup title="Font">
          <SettingRow label="Face" group>
            <SegmentedControl<ReadingFace>
              aria-label="Face"
              items={FACE_ITEMS}
              value={prefs.fontFamily}
              onValueChange={(v) => onChange({ fontFamily: v })}
            />
          </SettingRow>
          {prefs.fontFamily === "custom" && (
            <>
              <SettingRow
                label="Custom font"
                help="An installed font. Bookerly stands in without it."
              >
                <TextField
                  placeholder="e.g. Literata"
                  defaultValue={prefs.customFont}
                  onBlur={(e) => onChange({ customFont: e.currentTarget.value })}
                  onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                />
              </SettingRow>
              <SettingRow label="Weight">
                <NumberStepper
                  value={t.fontWeight}
                  min={100}
                  max={900}
                  step={100}
                  onValueChange={set("fontWeight")}
                />
              </SettingRow>
              <SettingRow label="Tracking">
                <NumberStepper
                  value={t.letterSpacing}
                  min={-0.05}
                  max={0.2}
                  step={0.01}
                  format={{ maximumFractionDigits: 3 }}
                  onValueChange={set("letterSpacing")}
                />
              </SettingRow>
            </>
          )}
          <SettingRow label="Size">
            <NumberStepper
              value={t.fontSize}
              min={14}
              max={28}
              step={1}
              onValueChange={set("fontSize")}
            />
          </SettingRow>
          <SettingRow label="Bold weight">
            <NumberStepper
              value={t.fontWeightBold}
              min={100}
              max={900}
              step={100}
              onValueChange={set("fontWeightBold")}
            />
          </SettingRow>
        </SettingGroup>
        <SettingGroup title="Page">
          <SettingRow label="Line height">
            <NumberStepper
              value={t.lineHeight}
              min={1.2}
              max={2.4}
              step={0.1}
              onValueChange={set("lineHeight")}
            />
          </SettingRow>
          <SettingRow label="Page width">
            <NumberStepper
              value={t.pageWidth}
              min={40}
              max={90}
              step={2}
              onValueChange={set("pageWidth")}
            />
          </SettingRow>
          <SettingRow label="Smart quotes" group>
            <SegmentedControl<"on" | "off">
              aria-label="Smart quotes"
              items={ON_OFF}
              value={prefs.smartQuotes ? "on" : "off"}
              onValueChange={(v) => onChange({ smartQuotes: v === "on" })}
            />
          </SettingRow>
        </SettingGroup>
        {prefs.fontFamily !== "custom" && (
          <Button variant="ghost" onClick={() => onChange(resetReaderType(prefs))}>
            Reset to defaults
          </Button>
        )}
      </PopoverPopup>
    </Popover>
  );
}
