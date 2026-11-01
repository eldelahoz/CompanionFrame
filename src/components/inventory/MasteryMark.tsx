import { GameIcon } from "../icons/GameIcon";
import type { Translator } from "../../lib/i18n";
import type { MasteryMark as Mark } from "../../lib/inventory/masteryMark";

/** A small mastery state icon for an item that counts toward mastery; the text is its tooltip and accessible name. */
export default function MasteryMark({ mark, tr }: { mark: Mark | null | undefined; tr: Translator }) {
  if (!mark) return null;
  const label = tr(mark.status === "mastered" ? "inventory.mastery.mastered" : mark.status === "progress" ? "inventory.mastery.progress" : "inventory.mastery.missing", { rank: mark.rank, max: mark.maxRank });
  return (
    <span className={`inv-mastery inv-mastery--${mark.status}`} role="img" aria-label={label} title={label} data-mastery={mark.status}>
      <GameIcon name="mastery" />
      {mark.status === "progress" && <span className="inv-mastery-rank" aria-hidden="true">{mark.rank}/{mark.maxRank}</span>}
    </span>
  );
}
