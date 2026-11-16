import { Palette } from "lucide-react";
import {
  Button,
  Icon,
  Popover,
  PopoverPopup,
  PopoverTrigger,
  ThemeSwitch,
} from "@lhak/style/react";

/// The kit's flavor and mode controls, in a popover where the old theme
/// toggle sat: they belong in an appearance panel, not on the toolbar itself.
export function AppearanceMenu() {
  return (
    <Popover>
      <PopoverTrigger render={<Button variant="ghost" iconOnly aria-label="Appearance" />}>
        <Icon icon={Palette} />
      </PopoverTrigger>
      <PopoverPopup title="Appearance" align="end">
        <ThemeSwitch />
      </PopoverPopup>
    </Popover>
  );
}
