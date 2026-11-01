import { Copy, Store } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import { rivenNameSuffix } from "../../lib/marketContract";
import { rivenChatTag, rivenWtsLine } from "../../lib/rivenChatTag";
import { attrGradeColor, gradeColor } from "../../lib/rivenGradeColors";
import { RIVEN_ATTR_GRADE_KEYS } from "../../lib/rivenLabels";
import { rivenDissolveHint } from "../../lib/rivens/dissolve";
import type { DecodedRiven, WfmContract } from "../../lib/ipc";
import { GameIcon, type GameIconName } from "../icons";
import { Button, IconButton, Readout } from "../ui";
import RivenPolarityIcon from "./RivenPolarityIcon";

export type RivenCardSize = "full" | "compact";

const DAMAGE: Record<string, GameIconName> = {
  cold: "damage-cold",
  electricity: "damage-electricity",
  heat: "damage-heat",
  impact: "damage-impact",
  puncture: "damage-puncture",
  slash: "damage-slash",
  toxin: "damage-toxin",
};
const damageIcon = (statName: string): GameIconName | null => {
  const lower = statName.toLowerCase();
  const hit = Object.keys(DAMAGE).find((k) => lower.includes(k));
  return hit ? DAMAGE[hit] : null;
};

export interface RivenCardProps {
  riven: DecodedRiven;
  listing?: WfmContract;
  size: RivenCardSize;
  tr: Translator;
  onOpen: () => void;
  onMenu: (e: React.MouseEvent) => void;
  onCopy: (text: string) => void;
}

function GradeBadges({ riven, tr }: { riven: DecodedRiven; tr: Translator }) {
  const attrKey = RIVEN_ATTR_GRADE_KEYS[riven.attributeGrade];
  return (
    <>
      <span style={{ color: gradeColor(riven.overallGrade) }} data-riven-grade={riven.overallGrade}>
        {riven.overallGrade}
      </span>
      {attrKey ? (
        <small style={{ color: attrGradeColor(riven.attributeGrade) }} title={tr("rivens.sort.attributeGrade")} data-riven-attr-grade={riven.attributeGrade}>
          {tr(attrKey)}
        </small>
      ) : (
        riven.attributeGrade === "?" && (
          <small className="text-text-muted" title={tr("rivens.detail.noGoodRollData")} data-riven-attr-grade="?">
            {tr("rivens.grade.unrated")}
          </small>
        )
      )}
    </>
  );
}

function StatRows({ riven, className = "" }: { riven: DecodedRiven; className?: string }) {
  return (
    <ul className={`riven-stats ${className}`}>
      {riven.stats.map((stat) => {
        const icon = damageIcon(stat.name);
        return (
          <li key={stat.tag} className={`riven-stat ${stat.positive ? "riven-stat--pos" : "riven-stat--neg"}`}>
            <b>{stat.multiplier ? `x${stat.displayValue}` : `${stat.displayValue >= 0 ? "+" : ""}${stat.displayValue}%`}</b>
            {icon && <GameIcon name={icon} />}
            <span>{stat.name}</span>
          </li>
        );
      })}
    </ul>
  );
}

function Actions({ riven, listing, tr, onCopy }: Pick<RivenCardProps, "riven" | "listing" | "tr" | "onCopy">) {
  return (
    <div className="riven-actions">
      {listing && (
        <span className="ui-badge ui-badge--accent" title={tr("rivens.listedPrice", { plat: listing.platinum })} data-riven-listed>
          <Store aria-label={tr("rivens.listedPrice", { plat: listing.platinum })} />
        </span>
      )}
      <IconButton label={tr("rivens.copyChatTag")} onClick={() => onCopy(rivenChatTag(riven))} data-riven-copy-tag>
        <Copy />
      </IconButton>
      {listing && (
        <Button size="sm" title={tr("rivens.copyWtsLine")} aria-label={tr("rivens.copyWtsLine")} onClick={() => onCopy(rivenWtsLine(riven, listing.buyoutPlatinum ?? listing.platinum))} data-riven-copy-wts>
          WTS
        </Button>
      )}
    </div>
  );
}

export default function RivenCard({ riven, listing, size, tr, onOpen, onMenu, onCopy }: RivenCardProps) {
  const suffix = rivenNameSuffix(riven.rivenName, riven.weaponName);
  const endo = rivenDissolveHint(riven);
  const endoTitle = endo !== null ? tr("rivens.dissolveValue", { endo }) : undefined;

  if (size === "full") {
    return (
      <div className="riven-full" data-riven-card={riven.itemId}>
        <button type="button" className="riven-full-art" aria-label={riven.rivenName || riven.weaponName} onClick={onOpen} onContextMenu={onMenu}>
          <span className="riven-full-grade">
            <GradeBadges riven={riven} tr={tr} />
          </span>
          {endo !== null && (
            <span className="riven-full-endo" title={endoTitle} data-riven-dissolve-endo={endo}>
              <GameIcon name="endo" />
              {endo}
            </span>
          )}
          <span className="riven-full-title">
            <strong>{riven.weaponName}</strong> {suffix && <span>{suffix}</span>}
          </span>
          <StatRows riven={riven} className="riven-full-stats" />
          <span className="riven-full-meta riven-meta">
            <span>{tr("rivens.mr", { level: riven.masteryReq })}</span>
            <RivenPolarityIcon polarity={riven.polarity} tr={tr} />
            <span>{tr("rivens.rerollCount", { count: riven.rerolls })}</span>
          </span>
          <span className="riven-full-pips" aria-hidden="true">
            {Array.from({ length: riven.maxRank }, (_, i) => (
              <i key={i} className={`riven-pip ${i < riven.currentRank ? "is-on" : ""}`} />
            ))}
          </span>
        </button>
        <Actions riven={riven} listing={listing} tr={tr} onCopy={onCopy} />
      </div>
    );
  }

  return (
    <div className="riven-card" data-riven-card={riven.itemId}>
      <button type="button" className="ui-card riven-card-main" onClick={onOpen} onContextMenu={onMenu}>
        <span className="riven-card-head">
          <span className="riven-card-name">
            <strong>{riven.weaponName}</strong>
            {suffix && <span>{suffix}</span>}
          </span>
          <span className="riven-grade">
            <GradeBadges riven={riven} tr={tr} />
          </span>
        </span>
        <StatRows riven={riven} />
        <span className="riven-meta">
          <span>{tr("rivens.mr", { level: riven.masteryReq })}</span>
          <RivenPolarityIcon polarity={riven.polarity} tr={tr} />
          <span className="riven-reroll">{tr("rivens.rerollCount", { count: riven.rerolls })}</span>
          {endo !== null && (
            <span title={endoTitle} data-riven-dissolve-endo={endo}>
              <Readout currency="endo" value={endo} />
            </span>
          )}
        </span>
      </button>
      <Actions riven={riven} listing={listing} tr={tr} onCopy={onCopy} />
    </div>
  );
}
