import type { Translator } from "../lib/i18n";

interface Props {
  text: string;
  slug: string | null;
  tr: Translator;
  title?: string;
}

export default function MarketPrice({ text, slug, tr, title = "Warframe.market" }: Props) {
  return (
    <div className="detail-section detail-market-section">
      <h3>{title}</h3>
      <div className="detail-market-body">
        {text || "..."}
        {slug && (
          <>
            <br />
            <a
              className="ui-btn ui-btn--sm mt-2"
              href={`https://warframe.market/items/${slug}`}
              target="_blank"
              rel="noreferrer noopener"
            >
              {tr("common.openOnWarframeMarket")}
            </a>
          </>
        )}
      </div>
    </div>
  );
}
