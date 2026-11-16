import { TYPE } from "@lhak/style";
import { definePersistedPref } from "./persistedPref";

/** Bookerly (`serif`) or Work Sans (`sans`) at their own settings, or a typed `custom` font. */
export type ReadingFace = "serif" | "sans" | "custom";

export interface ReaderPrefs {
  /** The blob's shape: 2 since adoption (absent before; see `deserialize`). */
  schema: 2;
  fontFamily: ReadingFace;
  /** Font family name typed by the user (e.g. "Literata"), used on `custom`.
   * Falls back to Bookerly if the named font isn't installed locally (no
   * fonts are downloaded/bundled). Kept while a face is chosen. */
  customFont: string;
  fontSize: number; // px
  /** 100-900, regular prose text on `custom`. `null` takes Bookerly's. A face uses its own. */
  fontWeight: number | null;
  fontWeightBold: number; // 100-900, applies to <b>/<strong> within prose
  lineHeight: number;
  pageWidth: number; // ch units
  /** em, on `custom`. `null` takes Bookerly's. A face uses its own. */
  letterSpacing: number | null;
  /** Convert straight quotes/apostrophes to curly ones at render time. */
  smartQuotes: boolean;
}

// Defaults are the @lhak/style reading contract (wayfinder 009). Adoption
// changed only these: a value the reader saved is never reset (adoption SPEC,
// "Reader settings").
const DEFAULTS: ReaderPrefs = {
  schema: 2,
  fontFamily: "serif",
  customFont: "",
  fontSize: TYPE.reading.size,
  fontWeight: null,
  fontWeightBold: 700,
  lineHeight: TYPE.reading.lineHeight,
  pageWidth: TYPE.reading.measure,
  letterSpacing: null,
  smartQuotes: true,
};

// Merges over DEFAULTS on every read (not just when nothing's stored) — a
// blob saved by an older app version missing a newer field still gets that
// field's default, rather than `undefined`. Before adoption a typed font name
// took priority over the face, so an old blob holding one opens on Custom,
// where its saved weight and tracking still apply.
function deserialize(raw: string): ReaderPrefs {
  const saved = JSON.parse(raw) as Partial<ReaderPrefs> & { schema?: number };
  const migrated =
    saved.schema === undefined && saved.customFont?.trim() ? { fontFamily: "custom" as const } : {};
  return { ...DEFAULTS, ...saved, ...migrated, schema: 2 };
}

const pref = definePersistedPref<ReaderPrefs>("ffn-reader-typography", DEFAULTS, { deserialize });

export const getReaderPrefs = pref.get;
export const saveReaderPrefs = pref.save;

/** Size, line-height, bold weight and page width back at their defaults; the rest kept. */
export function resetReaderType(prefs: ReaderPrefs): ReaderPrefs {
  const { fontSize, lineHeight, fontWeightBold, pageWidth } = DEFAULTS;
  return { ...prefs, fontSize, lineHeight, fontWeightBold, pageWidth };
}

/** What the reader sets the text in: the saved prefs with each unset value resolved. */
export interface ReaderTypography {
  /** CSS font-family. */
  fontFamily: string;
  /** CSS font-family for italics: the face's partner, or the typed font's own italic. */
  italicFamily: string;
  fontSize: number;
  fontWeight: number;
  fontWeightBold: number;
  lineHeight: number;
  letterSpacing: number;
  pageWidth: number;
}

export function readerTypography(prefs: ReaderPrefs): ReaderTypography {
  const shared = {
    fontSize: prefs.fontSize,
    fontWeightBold: prefs.fontWeightBold,
    lineHeight: prefs.lineHeight,
    pageWidth: prefs.pageWidth,
  };
  if (prefs.fontFamily === "sans" || prefs.fontFamily === "serif") {
    const name = prefs.fontFamily === "sans" ? "reading-sans" : "reading-serif";
    const face = TYPE.faces[name];
    return {
      ...shared,
      fontFamily: `var(--lk-font-${name})`,
      italicFamily: `var(--lk-italic-${name})`,
      fontWeight: face.weight,
      letterSpacing: parseFloat(face.tracking),
    };
  }
  // Custom: the typed font over Bookerly, at the reader's weight and tracking.
  const fallback = TYPE.faces["reading-serif"];
  const custom = prefs.customFont.trim();
  const fontFamily = custom
    ? `"${custom}", var(--lk-font-reading-serif)`
    : "var(--lk-font-reading-serif)";
  return {
    ...shared,
    fontFamily,
    italicFamily: custom ? fontFamily : "var(--lk-italic-reading-serif)",
    fontWeight: prefs.fontWeight ?? fallback.weight,
    letterSpacing: prefs.letterSpacing ?? parseFloat(fallback.tracking),
  };
}
