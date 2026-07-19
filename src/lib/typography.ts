// Converts straight quotes to curly (typographic) quotes. Operates only on
// text nodes of parsed HTML, not the raw markup string, so it can't mangle
// tag attributes (e.g. class="don't-do-this") the way a blind regex over
// the whole HTML would.

const ELISION = /'(?=(tis|twas|twill|em|cause|round|n'|\d\d(s|'s))\b)/gi;
const OPENING_DOUBLE = /(^|[-—([{“\s])"/g;
const OPENING_SINGLE = /(^|[-—([{“‘\s])'/g;

export function smartenQuotesText(text: string): string {
  return text
    .replace(ELISION, "’") // 'tis, 'em, '80s — leading elision curls like a closing quote
    .replace(OPENING_DOUBLE, "$1“")
    .replace(/"/g, "”")
    .replace(OPENING_SINGLE, "$1‘")
    .replace(/'/g, "’");
}

const SKIP_TAGS = new Set(["SCRIPT", "STYLE", "PRE", "CODE"]);

export function smartenQuotesHtml(html: string): string {
  if (typeof DOMParser === "undefined") return html;
  const doc = new DOMParser().parseFromString(html, "text/html");
  const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  let n: Node | null;
  while ((n = walker.nextNode())) {
    if (n.parentElement && SKIP_TAGS.has(n.parentElement.tagName)) continue;
    nodes.push(n as Text);
  }
  for (const node of nodes) {
    node.textContent = smartenQuotesText(node.textContent ?? "");
  }
  return doc.body.innerHTML;
}
