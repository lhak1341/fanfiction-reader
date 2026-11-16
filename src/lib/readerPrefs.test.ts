import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getReaderPrefs, readerTypography, resetReaderType, saveReaderPrefs } from "./readerPrefs";

// Plain Node: no `localStorage`, so a Map stands in (docs/TESTING.md).
function stubLocalStorage(saved?: object) {
  const store = new Map<string, string>();
  if (saved) store.set("ffn-reader-typography", JSON.stringify(saved));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
}

// The blob the deployed app saved before adoption (read from its WebKit
// localStorage on 2026-11-16): every field set, at the old Svelte defaults or
// the user's own choices. No `schema`: the pre-adoption shape.
const SAVED_BEFORE_ADOPTION = {
  fontFamily: "serif",
  customFont: "Bookerly",
  fontSize: 28,
  fontWeight: 400,
  fontWeightBold: 700,
  lineHeight: 1.35,
  pageWidth: 90,
  letterSpacing: 0,
  smartQuotes: true,
};

const prefsOfSaved = (saved?: object) => {
  stubLocalStorage(saved);
  return getReaderPrefs();
};
const typographyOfSaved = (saved?: object) => readerTypography(prefsOfSaved(saved));

describe("reader settings across adoption", () => {
  afterEach(() => vi.unstubAllGlobals());

  describe("a saved setting is kept", () => {
    it("keeps the saved size", () => {
      expect(typographyOfSaved({ fontSize: 17 }).fontSize).toBe(17);
    });
    it("keeps the saved line-height", () => {
      expect(typographyOfSaved({ lineHeight: 1.75 }).lineHeight).toBe(1.75);
    });
    it("keeps the saved weight, on a custom font", () => {
      expect(typographyOfSaved({ customFont: "Literata", fontWeight: 500 }).fontWeight).toBe(500);
    });
    it("keeps the saved tracking, even zero, on a custom font", () => {
      const t = typographyOfSaved({ customFont: "Literata", letterSpacing: 0 });
      expect(t.letterSpacing).toBe(0);
    });
    it("opens the pre-adoption blob on Custom, every value kept", () => {
      const prefs = prefsOfSaved(SAVED_BEFORE_ADOPTION);
      expect(prefs.fontFamily).toBe("custom");
      expect(readerTypography(prefs)).toMatchObject({
        fontFamily: '"Bookerly", var(--lk-font-reading-serif)',
        fontSize: 28,
        fontWeight: 400,
        fontWeightBold: 700,
        lineHeight: 1.35,
        letterSpacing: 0,
        pageWidth: 90,
      });
    });
    it("keeps Serif when a blob saved after adoption still holds a font name", () => {
      expect(
        prefsOfSaved({ schema: 2, fontFamily: "serif", customFont: "Literata" }).fontFamily
      ).toBe("serif");
    });
  });

  describe("an unset setting takes the contract default", () => {
    it("sets reading text at 24px", () => {
      expect(typographyOfSaved().fontSize).toBe(24);
    });
    it("sets line-height 1.45", () => {
      expect(typographyOfSaved().lineHeight).toBe(1.45);
    });
    it("sets a 58ch column", () => {
      expect(typographyOfSaved().pageWidth).toBe(58);
    });
    it("fills a field an older blob lacks, keeping the rest", () => {
      const { fontSize, lineHeight } = typographyOfSaved({ fontSize: 20 });
      expect([fontSize, lineHeight]).toEqual([20, 1.45]);
    });
    it("gives a custom font Bookerly's weight and tracking until the reader sets them", () => {
      expect(typographyOfSaved({ fontFamily: "custom", customFont: "Literata" })).toMatchObject({
        fontWeight: 400,
        letterSpacing: 0,
      });
    });
  });

  describe("faces bring their own settings", () => {
    it("sets Serif in Bookerly at 400 and 0em, italics in Bookerly's own", () => {
      expect(typographyOfSaved({ schema: 2, fontFamily: "serif" })).toMatchObject({
        fontFamily: "var(--lk-font-reading-serif)",
        italicFamily: "var(--lk-italic-reading-serif)",
        fontWeight: 400,
        letterSpacing: 0,
      });
    });
    it("sets Sans in Work Sans at 430 and -0.02em, italics in its partner", () => {
      expect(typographyOfSaved({ schema: 2, fontFamily: "sans" })).toMatchObject({
        fontFamily: "var(--lk-font-reading-sans)",
        italicFamily: "var(--lk-italic-reading-sans)",
        fontWeight: 430,
        letterSpacing: -0.02,
      });
    });
    it("ignores a weight and tracking saved for Custom while on a face", () => {
      const t = typographyOfSaved({
        schema: 2,
        fontFamily: "sans",
        fontWeight: 600,
        letterSpacing: 0.1,
      });
      expect([t.fontWeight, t.letterSpacing]).toEqual([430, -0.02]);
    });
    it("puts a custom font first, over Bookerly, with its own italic", () => {
      expect(typographyOfSaved({ fontFamily: "custom", customFont: " Literata " })).toMatchObject({
        fontFamily: '"Literata", var(--lk-font-reading-serif)',
        italicFamily: '"Literata", var(--lk-font-reading-serif)',
      });
    });
    it("reads Custom with no font name as Bookerly", () => {
      expect(typographyOfSaved({ fontFamily: "custom", customFont: "" }).fontFamily).toBe(
        "var(--lk-font-reading-serif)"
      );
    });
  });

  describe("reset to defaults", () => {
    it("puts size, line-height, bold weight and page width back, and nothing else", () => {
      const saved = prefsOfSaved({
        schema: 2,
        fontFamily: "sans",
        customFont: "Literata",
        fontSize: 28,
        lineHeight: 1.35,
        fontWeightBold: 800,
        pageWidth: 90,
        fontWeight: 600,
        letterSpacing: 0.1,
        smartQuotes: false,
      });
      expect(resetReaderType(saved)).toEqual({
        ...saved,
        fontSize: 24,
        lineHeight: 1.45,
        fontWeightBold: 700,
        pageWidth: 58,
      });
    });
  });

  describe("saving", () => {
    beforeEach(() => stubLocalStorage());
    it("marks a saved blob as the new shape, so a leftover font name stays unused", () => {
      saveReaderPrefs({ ...getReaderPrefs(), customFont: "Literata", fontSize: 26 });
      expect(getReaderPrefs()).toMatchObject({ fontFamily: "serif", fontSize: 26, schema: 2 });
    });
  });
});
