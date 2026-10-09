import { definePersistedPref } from "./persistedPref";

export interface ReaderPrefs {
  fontFamily: "serif" | "sans";
  /** Font family name typed by the user (e.g. "Bookerly") — takes priority
   * over `fontFamily` when non-empty; falls back to it if the named font
   * isn't installed locally (no fonts are downloaded/bundled). */
  customFont: string;
  fontSize: number; // px
  fontWeight: number; // 100-900, applies to regular prose text
  fontWeightBold: number; // 100-900, applies to <b>/<strong> within prose
  lineHeight: number;
  pageWidth: number; // ch units
  letterSpacing: number; // em
  /** Convert straight quotes/apostrophes to curly ones at render time. */
  smartQuotes: boolean;
}

const DEFAULTS: ReaderPrefs = {
  fontFamily: "serif",
  customFont: "",
  fontSize: 18,
  fontWeight: 400,
  fontWeightBold: 700,
  lineHeight: 1.75,
  pageWidth: 58,
  letterSpacing: 0,
  smartQuotes: true,
};

// Merges over DEFAULTS on every read (not just when nothing's stored) — a
// blob saved by an older app version missing a newer field still gets that
// field's default, rather than `undefined`.
const pref = definePersistedPref<ReaderPrefs>("ffn-reader-typography", DEFAULTS, {
  deserialize: (raw) => ({ ...DEFAULTS, ...JSON.parse(raw) }),
});

export const getReaderPrefs = pref.get;
export const saveReaderPrefs = pref.save;
