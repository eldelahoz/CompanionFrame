/** "2 hr. ago", "3 days ago" in the locale; empty when the date is missing or unreadable. */
export function orderAge(iso: string | null | undefined, now: number, locale: string): string {
  const t = Date.parse(iso ?? "");
  if (Number.isNaN(t)) return "";
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "always", style: "short" });
  const minutes = Math.max(0, Math.floor((now - t) / 60_000));
  if (minutes < 60) return format.format(-minutes, "minute");
  if (minutes < 1440) return format.format(-Math.floor(minutes / 60), "hour");
  return format.format(-Math.floor(minutes / 1440), "day");
}
