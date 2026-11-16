// @vitest-environment jsdom
//
// Chapter HTML as the Reader renders it. The app's strict `style-src` (no
// 'unsafe-inline') drops every `style="…"` attribute in parsed HTML, and the
// saved chapters carry two kinds (2026-11-16 survey of longform.db: 570 of
// 2159 chapters; `text-align:center` 3194 times, underline 984 times).

import { describe, expect, it } from "vitest";
import { chapterHtml } from "./chapterHtml";

const plain = { smartQuotes: false };

describe("chapterHtml", () => {
  it("turns centred text into a class and drops the style attribute", () => {
    expect(chapterHtml(`<p style="text-align:center;">* * *</p>`, plain)).toBe(
      `<p class="fanf-align-center">* * *</p>`
    );
  });

  it("reads underline written either way, beside an existing class", () => {
    expect(
      chapterHtml(
        `<span class="x" style="text-decoration: underline;">a</span><span style="text-decoration:underline">b</span>`,
        plain
      )
    ).toBe(`<span class="x fanf-underline">a</span><span class="fanf-underline">b</span>`);
  });

  it("keeps every known declaration of one attribute", () => {
    expect(
      chapterHtml(`<p style="text-align: right; text-decoration: line-through">a</p>`, plain)
    ).toBe(`<p class="fanf-align-right fanf-strike">a</p>`);
  });

  it("drops a declaration it has no class for, as the CSP would", () => {
    expect(chapterHtml(`<p style="color: red">a</p>`, plain)).toBe(`<p>a</p>`);
  });

  it("curls quotes when asked, and only then", () => {
    expect(chapterHtml(`<p>"hi"</p>`, { smartQuotes: true })).toBe("<p>“hi”</p>");
    expect(chapterHtml(`<p>"hi"</p>`, plain)).toBe(`<p>"hi"</p>`);
  });
});
