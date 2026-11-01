import type { SyndicateMeta } from "../../config/shared/syndicateTypes";
import type { Translator } from "../lib/i18n";
import ItemImage from "./ItemImage";

const BROWSE_WF = "https://browse.wf";
const DE_CDN = "https://content.warframe.com/PublicExport";

/** Syndicate sigil from DE's public icon path (browse.wf, then DE's content CDN); the image placeholder when neither loads. */
export default function SyndicateEmblem({ meta, tr }: { meta: Pick<SyndicateMeta, "name" | "icon" | "iconHash">; tr: Translator }) {
  const src = meta.icon ? `${BROWSE_WF}${meta.icon}` : null;
  const fallback = meta.icon && meta.iconHash ? `${DE_CDN}${meta.icon}!${meta.iconHash}` : null;
  return <ItemImage src={src} fallback={fallback} alt={meta.name} cls="syndicate-emblem !h-8 !w-8 shrink-0" tr={tr} />;
}
