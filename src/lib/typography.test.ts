import { describe, expect, it } from "vitest";
import { smartenQuotesText } from "./typography";

describe("smartenQuotesText", () => {
  it("curls opening and closing double quotes", () => {
    expect(smartenQuotesText('She said "hello there."')).toBe("She said “hello there.”");
  });

  it("curls opening and closing single quotes", () => {
    expect(smartenQuotesText("He said 'hi.'")).toBe("He said ‘hi.’");
  });

  it("treats an apostrophe in a contraction as closing, not opening", () => {
    expect(smartenQuotesText("don't")).toBe("don’t");
  });

  it("handles nested quotes", () => {
    expect(smartenQuotesText(`She said "it's 'complicated.'"`)).toBe("She said “it’s ‘complicated.’”");
  });

  it("curls leading elision apostrophes as closing quotes, not opening", () => {
    expect(smartenQuotesText("'tis the season")).toBe("’tis the season");
    expect(smartenQuotesText("'em all")).toBe("’em all");
    expect(smartenQuotesText("the '80s")).toBe("the ’80s");
  });

  it("curls a possessive apostrophe at the end of a word", () => {
    expect(smartenQuotesText("the dogs' bones")).toBe("the dogs’ bones");
  });
});
