// @vitest-environment jsdom
//
// smartenQuotesHtml's real behavior (TreeWalker over parsed HTML, SKIP_TAGS,
// textContent mutation) needs a real DOMParser — typography.test.ts runs
// under vitest's default node environment, where DOMParser is undefined and
// smartenQuotesHtml always takes its SSR early-return. This file opts into
// jsdom so the DOM-walking logic Reader.svelte actually calls on every
// chapter render gets exercised, not just its regex-only fallback.

import { describe, expect, it } from "vitest";
import { smartenQuotesHtml } from "./typography";

describe("smartenQuotesHtml", () => {
  it("curls quotes inside a text node", () => {
    expect(smartenQuotesHtml(`<p>She said "hello there."</p>`)).toBe("<p>She said “hello there.”</p>");
  });

  it("curls quotes across multiple sibling and nested elements", () => {
    const input = `<p>He said 'hi.'</p><p>She replied, <em>"you too."</em></p>`;
    const output = `<p>He said ‘hi.’</p><p>She replied, <em>“you too.”</em></p>`;
    expect(smartenQuotesHtml(input)).toBe(output);
  });

  it("leaves text inside SCRIPT/STYLE/PRE/CODE untouched", () => {
    const input = `<p>"Real prose."</p><script>var x = "raw";</script><style>.a{content:"raw"}</style><pre>"raw"</pre><code>"raw"</code>`;
    const output = smartenQuotesHtml(input);
    expect(output).toContain("“Real prose.”");
    expect(output).toContain('var x = "raw";');
    expect(output).toContain('.a{content:"raw"}');
    expect(output).toContain(">\"raw\"</pre>");
    expect(output).toContain(">\"raw\"</code>");
  });

  it("doesn't mangle tag attributes, only text-node content", () => {
    const input = `<p class="don't-do-this">He said "hi."</p>`;
    expect(smartenQuotesHtml(input)).toBe(`<p class="don't-do-this">He said “hi.”</p>`);
  });
});
