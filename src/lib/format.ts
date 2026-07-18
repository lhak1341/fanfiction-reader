export function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

function toShort(year: number, month: number, day: number): string {
  const yy = String(year).slice(-2);
  const mm = String(month).padStart(2, "0");
  const dd = String(day).padStart(2, "0");
  return `${yy}-${mm}-${dd}`;
}

/**
 * Compact form of FFN's raw "Updated:" text — FFN uses three shapes
 * depending on recency, confirmed against live data: a relative age for
 * anything very recent ("2h"), bare "M/D" with no year for this year
 * ("6/20"), and full "M/D/YYYY" once it's old enough. `new Date(raw)` alone
 * can't parse the first two (relative text isn't a date at all; a bare
 * "M/D" is ambiguous without a year), so each shape is matched explicitly.
 * `null` if the text doesn't match any of them.
 */
export function shortDate(raw: string): string | null {
  if (/^\d+[a-zA-Z]+$/.test(raw)) return raw; // already compact, e.g. "2h"

  const mdy = raw.match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/);
  if (mdy) {
    const [, m, d, y] = mdy;
    return toShort(Number(y), Number(m), Number(d));
  }

  const md = raw.match(/^(\d{1,2})\/(\d{1,2})$/);
  if (md) {
    const [, m, d] = md;
    const now = new Date();
    let year = now.getFullYear();
    // A bare M/D that would land in the future (e.g. "12/30" read in
    // January) actually means last year's December, not this year's.
    if (new Date(year, Number(m) - 1, Number(d)).getTime() > now.getTime()) year -= 1;
    return toShort(year, Number(m), Number(d));
  }

  return null;
}
