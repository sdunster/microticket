const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

const UNITS: Array<[Intl.RelativeTimeFormatUnit, number]> = [
  ["year", 60 * 60 * 24 * 365],
  ["month", 60 * 60 * 24 * 30],
  ["week", 60 * 60 * 24 * 7],
  ["day", 60 * 60 * 24],
  ["hour", 60 * 60],
  ["minute", 60],
];

/**
 * Formats a unix-seconds timestamp (the API's `Int!` epoch fields) as a
 * relative time, e.g. "3 hours ago" — what a triage-focused ticket row
 * needs, not an absolute date. Falls back to "just now" under a minute.
 */
export function formatRelativeTime(
  epochSeconds: number,
  now: number = Date.now(),
): string {
  const diffSeconds = Math.round(epochSeconds - now / 1000);
  const absSeconds = Math.abs(diffSeconds);

  if (absSeconds < 60) {
    return "just now";
  }

  for (const [unit, secondsInUnit] of UNITS) {
    if (absSeconds >= secondsInUnit) {
      return rtf.format(Math.round(diffSeconds / secondsInUnit), unit);
    }
  }

  return rtf.format(Math.round(diffSeconds / 60), "minute");
}
