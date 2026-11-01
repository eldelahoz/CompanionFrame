export function toOfficialWikiUrl(url: string): string {
  try {
    const parsed = new URL(url);
    if (parsed.hostname === "warframe.fandom.com" || parsed.hostname === "www.warframe.fandom.com") {
      const page = parsed.pathname.replace(/^\/wiki\//, "");
      return `https://wiki.warframe.com/w/${page}${parsed.search}${parsed.hash}`;
    }
  } catch {
    /* invalid URL, return as-is */
  }
  return url;
}

export function buildWikiUrl(name: string): string {
  return `https://wiki.warframe.com/w/${encodeURIComponent(name.replace(/ /g, "_"))}`;
}
