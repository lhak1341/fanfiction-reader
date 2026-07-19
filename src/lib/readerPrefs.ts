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

const STORAGE_KEY = "ffn-reader-typography";

const DEFAULTS: ReaderPrefs = {
  fontFamily: "serif",
  customFont: "",
  fontSize: 18,
  fontWeight: 400,
  fontWeightBold: 700,
  lineHeight: 1.75,
  pageWidth: 58,
  letterSpacing: 0,
  smartQuotes: true
};

export function getReaderPrefs(): ReaderPrefs {
  if (typeof localStorage === "undefined") return DEFAULTS;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    return DEFAULTS;
  }
}

export function saveReaderPrefs(prefs: ReaderPrefs) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
}
