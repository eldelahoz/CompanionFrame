import { ExternalLink } from "lucide-react";
import type { Translator } from "../lib/i18n";
import { buildWikiUrl, toOfficialWikiUrl } from "../lib/wikiUrl";

interface Props {
  /** Canonical wiki URL when known. */
  wikiUrl?: string | null;
  /** Builds the Warframe Wiki URL when `wikiUrl` is null. */
  fallbackName: string;
  tr: Translator;
}

export default function WikiButton({ wikiUrl, fallbackName, tr }: Props) {
  return (
    <a className="ui-btn ui-btn--sm" href={wikiUrl ? toOfficialWikiUrl(wikiUrl) : buildWikiUrl(fallbackName)} target="_blank" rel="noreferrer noopener" title={tr("wiki.openOnWikiTitle")}>
      <ExternalLink aria-hidden="true" />
      <span>{tr("common.wiki")}</span>
    </a>
  );
}
