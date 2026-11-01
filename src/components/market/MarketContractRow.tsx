import { Eye, EyeOff, ExternalLink, Pencil, Trash2 } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import type { WfmContract } from "../../lib/ipc";
import { attributeKeyword } from "../../lib/marketContract";
import { listingWarning, type ListingInventoryMatch } from "../../lib/marketListing";
import type { RivenContractGrade } from "../../lib/rivenContractGrades";
import { attrGradeColor, gradeColor } from "../../lib/rivenGradeColors";
import { RIVEN_ATTR_GRADE_KEYS } from "../../lib/rivenLabels";

interface Props {
  contract: WfmContract;
  /** Undefined = not graded (no badge); null = the grader could not resolve the weapon. */
  grade?: RivenContractGrade | null;
  /** Whether the listing still lines up with the inventory; absent = no opinion. */
  inventoryMatch?: ListingInventoryMatch | null;
  busy: boolean;
  onEdit: (c: WfmContract) => void;
  onToggleVisible: (c: WfmContract) => void;
  onRemove: (c: WfmContract) => void;
  tr: Translator;
}

const titleCase = (s: string) => s.replace(/\b\w/g, (c) => c.toUpperCase());

import { Badge, IconButton, Readout } from "../ui";

export default function MarketContractRow({ contract, grade, inventoryMatch = null, busy, onEdit, onToggleVisible, onRemove, tr }: Props) {
  const stats = contract.stats.slice(0, 4).map(attributeKeyword).filter(Boolean).map(titleCase);
  const direct = contract.isDirectSell;
  const badge = tr(direct ? "market.badge.direct" : contract.buyoutPlatinum ? "common.auction" : "market.badge.listing");
  const attr = grade === undefined ? "" : (grade?.attributeGrade ?? "?");
  const attrKey = RIVEN_ATTR_GRADE_KEYS[attr];
  const warning = listingWarning(inventoryMatch, contract.modRank, tr);

  return (
    <tr data-market-contract={contract.id}>
      <td data-primary>
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <strong>{contract.itemName}</strong>
          {contract.modRank != null && <span className="text-xs text-text-muted">R{contract.modRank}</span>}
          {contract.rerolls != null && <span className="text-xs text-text-muted">RR{contract.rerolls}</span>}
          {!contract.visible && <Badge>{tr("market.hiddenLower")}</Badge>}
          {warning && (
            <span className="listing-warning" data-contract-warning title={warning.title}>
              {warning.label}
            </span>
          )}
        </div>
        {stats.length > 0 && <div className="mt-1 text-xs text-text-muted">{stats.join(", ")}</div>}
      </td>
      {grade !== undefined && (
        <td data-label={tr("rivens.sort.grade")}>
          <span className="flex flex-col gap-1 font-semibold leading-none" data-contract-grade={contract.id}>
            {grade?.overallGrade && (
              <span style={{ color: gradeColor(grade.overallGrade) }} data-riven-grade={grade.overallGrade}>
                {grade.overallGrade}
              </span>
            )}
            {attrKey ? (
              <small className="text-xs font-medium" style={{ color: attrGradeColor(attr) }} title={tr("rivens.sort.attributeGrade")} data-riven-attr-grade={attr}>
                {tr(attrKey)}
              </small>
            ) : (
              <small className="text-xs font-medium text-text-muted" title={tr("rivens.detail.noGoodRollData")} data-riven-attr-grade="?">
                {tr("rivens.grade.unrated")}
              </small>
            )}
          </span>
        </td>
      )}
      <td data-label={tr("common.platinum")} title={tr("common.platinum")}>
        <Readout currency="platinum" value={contract.platinum} chip />
      </td>
      <td data-label="MR" className="text-text-secondary">
        {contract.masteryLevel != null ? `MR${contract.masteryLevel}` : "MR-"}
        {contract.polarity && <span className="ml-2 capitalize text-text-muted">{contract.polarity}</span>}
      </td>
      <td data-label={tr("common.type")}>
        <Badge tone={direct ? "warning" : "info"}>{badge}</Badge>
      </td>
      <td data-actions>
        <div className="flex items-center justify-end gap-1">
          <IconButton label={tr("market.edit")} disabled={busy} onClick={() => onEdit(contract)}>
            <Pencil />
          </IconButton>
          <a className="ui-icon-btn" href={contract.listingUrl} target="_blank" rel="noreferrer" aria-label={tr("market.open")} data-tip={tr("market.open")}>
            <ExternalLink />
          </a>
          <IconButton label={tr(contract.visible ? "market.riven.hideListing" : "market.riven.showListing")} disabled={busy} data-contract-visible onClick={() => onToggleVisible(contract)}>
            {contract.visible ? <EyeOff /> : <Eye />}
          </IconButton>
          <IconButton label={tr("market.riven.removeListing")} danger disabled={busy} data-contract-remove onClick={() => onRemove(contract)}>
            <Trash2 />
          </IconButton>
        </div>
      </td>
    </tr>
  );
}
