/**
 * Calendar-date helpers for invoicing's `YYYY-MM-DD` strings. These are
 * dates, not instants — never round-tripped through `Date` parsing, which
 * would read `"2026-08-19"` as UTC midnight and shift it a day west of UTC.
 */

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/** Today in the browser's own timezone, as `YYYY-MM-DD`. */
export function localToday(now: Date = new Date()): string {
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** `"2026-08-19"` → `"19/08/2026"`, the invoice's date format. Anything
 * that isn't `YYYY-MM-DD` comes back unchanged. */
export function formatDate(isoDate: string): string {
  const match = ISO_DATE.exec(isoDate);
  if (!match) return isoDate;
  return `${match[3]}/${match[2]}/${match[1]}`;
}
