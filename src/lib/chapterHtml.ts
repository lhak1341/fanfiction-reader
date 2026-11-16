// A fetched chapter's HTML, made ready for the Reader. Two passes over one
// parse: smart quotes (a reader setting), and the inline `style` attributes
// rewritten as classes. The app's `style-src` has no 'unsafe-inline', so the
// WebView would drop every `style="…"` in the chapter: centred scene breaks
// and underlines, the only declarations the saved chapters hold (survey,
// 2026-11-16), keep their look through `.fanf-prose` classes in app.css.

import { smartenQuotesIn } from "./typography";

// `property: value` → class. A declaration not listed is dropped, as the CSP
// would drop it.
const CLASS_FOR: Record<string, string> = {
  "text-align: center": "fanf-align-center",
  "text-align: right": "fanf-align-right",
  "text-align: left": "fanf-align-left",
  "text-align: justify": "fanf-align-justify",
  "text-decoration: underline": "fanf-underline",
  "text-decoration: line-through": "fanf-strike",
};

function classesFor(style: string): string[] {
  return style
    .split(";")
    .map((d) => d.split(":").map((s) => s.trim().toLowerCase()))
    .filter((parts) => parts.length === 2)
    .map(([prop, value]) => CLASS_FOR[`${prop}: ${value}`])
    .filter((c): c is string => c !== undefined);
}

export function chapterHtml(html: string, { smartQuotes }: { smartQuotes: boolean }): string {
  if (typeof DOMParser === "undefined") return html;
  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const el of doc.body.querySelectorAll("[style]")) {
    el.classList.add(...classesFor(el.getAttribute("style") ?? ""));
    el.removeAttribute("style");
    if (el.classList.length === 0) el.removeAttribute("class");
  }
  if (smartQuotes) smartenQuotesIn(doc.body);
  return doc.body.innerHTML;
}
