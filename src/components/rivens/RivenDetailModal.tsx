import { useEffect, useMemo, useState } from "react";
import type { DecodedRiven } from "../../bindings";
import { tagToWfmUrlName, polarityToWfm } from "../../../config/shared/wfmRivenVocabulary";
import type { MessageKey, Translator } from "../../lib/i18n";
import { rivenBestAttributes, rivensGoodRollsInfo, rivensRefreshGoodRolls, wfmRivenAuctionCreate, wfmRivenAuctionUpdate, wfmRivenSearch, wfmSessionStatus, type RivenBestAttributes, type RivenGoodRollsInfo, type WfmContract } from "../../lib/ipc";
import { familySlug, rivenNameSuffix } from "../../lib/marketContract";
import { attrGradeColor, dispoStars, gradeColor } from "../../lib/rivenGradeColors";
import { RIVEN_ATTR_GRADE_KEYS, RIVEN_TYPE_KEYS } from "../../lib/rivenLabels";
import { canonicalRivenStatName, computeRivenStatSimilarity, formatRivenListingStatValue } from "../../lib/rivenSimilarity";
import { rivenDissolveHint } from "../../lib/rivens/dissolve";
import { Badge, Button, Checkbox, Chip, Field, Input, Modal, Readout, Segmented, Select } from "../ui";
import { RefreshCw } from "lucide-react";

// The bindings type every f64 as number | null (NaN serializes to null); the decoder never emits one.
type Finite<T> = [T] extends [number | null] ? ([T] extends [null] ? null : number) : T extends (infer U)[] ? Finite<U>[] : T extends object ? { [K in keyof T]: Finite<T[K]> } : T;

interface Props {
  riven: DecodedRiven;
  contract: WfmContract | null;
  tr: Translator;
  locale: string;
  /** Weapon's vaulted flag from the item database; the tag is hidden when unknown. */
  vaulted?: boolean;
  /** Persisted app setting: show auction listings among the similar rivens. */
  similarAuctions: boolean;
  onSimilarAuctions: (shown: boolean) => void;
  onContractUpdated: () => void;
  onClose: () => void;
}

const DEFAULT_LISTING_COUNT = 20;
/** WFM stores the note as HTML-ish text. */
const plainNote = (note: string | null | undefined) =>
  String(note ?? "").replace(/<br\s*\/?>/gi, "\n").replace(/<\/p>/gi, "\n").replace(/<[^>]+>/g, "").trim();

const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : Number(v) || 0);
const askedPrice = (l: WfmContract) => l.buyoutPlatinum ?? l.startingPlatinum ?? l.platinum;

type Similar = { listing: WfmContract; pct: number; matchedNames: Set<string> };

/** "Updated 3d ago" for the good-roll sheet. */
const ageLabel = (iso: string | null, tr: Translator): string => {
  const ms = iso ? Date.now() - Date.parse(iso) : NaN;
  if (!Number.isFinite(ms)) return tr("rivens.detail.dictionaryNever");
  const [sec, min, hr, days] = [ms / 1e3, ms / 6e4, ms / 36e5, ms / 864e5].map(Math.floor);
  if (sec < 10) return tr("common.updatedJustNow");
  if (min < 1) return tr("common.updatedSAgo", { sec });
  if (hr < 1) return tr("common.updatedMAgo", { min });
  return days < 1 ? tr("common.updatedHAgo", { hr }) : tr("common.updatedDAgo", { days });
};

export default function RivenDetailModal({ riven: rivenIn, contract, tr, vaulted, similarAuctions: showAuctions, onSimilarAuctions, onContractUpdated, onClose }: Props) {
  const riven = rivenIn as Finite<DecodedRiven>;
  const isContract = contract != null;
  const isAuction = contract?.isDirectSell === false;
  const [similar, setSimilar] = useState<Similar[]>([]);
  const [loadingListings, setLoadingListings] = useState(true);
  const [showAll, setShowAll] = useState(false);
  const [bestAttrs, setBestAttrs] = useState<RivenBestAttributes | null>(null);
  const [rolls, setRolls] = useState<RivenGoodRollsInfo | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [loggedIn, setLoggedIn] = useState(false);

  const [type, setType] = useState<"direct" | "auction">(isAuction ? "auction" : "direct");
  const [visibility, setVisibility] = useState<"public" | "private">(contract?.visible === false ? "private" : "public");
  const [description, setDescription] = useState(plainNote(contract?.note));
  // An auction's own field is the starting bid; `platinum` can be the buyout.
  const [price, setPrice] = useState((isAuction ? contract?.startingPlatinum : null) ?? contract?.platinum ?? 0);
  /** Auctions only; 0 means "no buyout", which WFM accepts. */
  const [buyout, setBuyout] = useState(isAuction ? (contract?.buyoutPlatinum ?? 0) : 0);
  const [minRep, setMinRep] = useState(isAuction ? (contract?.minimalReputation ?? 0) : 0);
  const [rank, setRank] = useState(riven.currentRank);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");

  useEffect(() => {
    let gone = false;
    const myNames = riven.stats.map((s) => s.name.toLowerCase());
    wfmRivenSearch(familySlug(riven.weaponName))
      .then((listings) => {
        if (gone) return;
        const enriched = listings.map((listing) => ({ listing, ...computeRivenStatSimilarity(myNames, listing.stats.map((s) => ({ name: s.label }))) }));
        enriched.sort((a, b) => b.pct - a.pct || askedPrice(a.listing) - askedPrice(b.listing));
        setSimilar(enriched.filter((e) => e.pct >= 25));
      })
      .catch(() => !gone && setSimilar([]))
      .finally(() => !gone && setLoadingListings(false));
    wfmSessionStatus().then((s) => !gone && setLoggedIn(s.logged_in)).catch(() => {});
    rivenBestAttributes(riven.weaponName, riven.rivenType === "Melee" || riven.rivenType === "Zaw")
      .then((r) => !gone && setBestAttrs(r))
      .catch(() => !gone && setBestAttrs(null));
    rivensGoodRollsInfo().then((r) => !gone && setRolls(r)).catch(() => {});
    return () => {
      gone = true;
    };
  }, [riven.weaponName, riven.rivenType, riven.stats]);

  const refreshRolls = async () => {
    setRefreshing(true);
    try {
      setRolls(await rivensRefreshGoodRolls());
      setBestAttrs(await rivenBestAttributes(riven.weaponName, riven.rivenType === "Melee" || riven.rivenType === "Zaw"));
    } catch {
      // keeps the previous sheet; an unchanged age label tells the user nothing was refreshed
    } finally {
      setRefreshing(false);
    }
  };

  const positives = riven.stats.filter((s) => s.positive);
  const myPositive = useMemo(() => new Set(positives.map((s) => canonicalRivenStatName(s.name))), [positives]);
  const shown = showAuctions ? similar : similar.filter((e) => e.listing.isDirectSell !== false);
  const visible = showAll ? shown : shown.slice(0, DEFAULT_LISTING_COUNT);
  const typeKey = RIVEN_TYPE_KEYS[riven.rivenType];
  const attrKey = RIVEN_ATTR_GRADE_KEYS[riven.attributeGrade];
  const attrLabel = attrKey ? tr(attrKey) : riven.attributeGrade === "?" ? tr("rivens.grade.unrated") : riven.attributeGrade;
  const hasRoll = riven.overallGrade !== "";
  const hasAttr = riven.attributeGrade !== "";
  const dissolveEndo = rivenDissolveHint(riven);
  const canListAtMax = !isContract && riven.currentRank < riven.maxRank;
  const fail = (key: MessageKey) => setError(tr(key));

  const submit = async () => {
    if (busy) return;
    if (price < 1) return fail(type === "auction" ? "rivens.detail.startingBidMin" : "rivens.detail.priceMin");
    if (type === "auction" && buyout > 0 && buyout < price) return fail("rivens.detail.buyoutBelowStarting");
    if (type === "auction" && minRep < 0) return fail("rivens.detail.minRepNegative");
    setBusy(true);
    setError("");
    setSuccess("");
    // Direct sale is a buyout-only listing; an auction's buyout is optional.
    const buyoutPrice = type === "direct" ? price : buyout > 0 ? buyout : null;
    const minReputation = type === "auction" ? minRep : 0;
    try {
      if (contract) {
        await wfmRivenAuctionUpdate(contract.id, { buyoutPrice, startingPrice: price, minReputation, isPrivate: visibility === "private", description });
        setSuccess(tr("rivens.detail.contractUpdated"));
      } else {
        await wfmRivenAuctionCreate({
          weaponSlug: familySlug(riven.weaponName),
          rivenName: rivenNameSuffix(riven.rivenName, riven.weaponName),
          attributes: riven.stats.map((s) => ({
            urlName: tagToWfmUrlName(s.tag) ?? "",
            value: s.rankValues?.[rank] ?? (rank === riven.maxRank ? s.maxRankValue : s.displayValue),
            positive: s.positive,
          })),
          rerolls: riven.rerolls,
          masteryLevel: riven.masteryReq,
          polarity: polarityToWfm(riven.polarity) ?? riven.polarity,
          modRank: rank,
          buyoutPrice,
          startingPrice: price,
          minReputation,
          isPrivate: visibility === "private",
          description,
        });
        setSuccess(tr("rivens.detail.listedOnWfm"));
      }
      onContractUpdated();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const priceField = (value: number, set: (n: number) => void, label: string, min: number) => (
    <Readout currency="platinum" value={null}>
      <Input type="number" value={value || ""} min={min} aria-label={label} onChange={(e) => set(num(e.target.value))} />
    </Readout>
  );

  const gradeCell = (title: string, value: string, color: string, sub: React.ReactNode, attr: Record<string, string>) => (
    <div className="riven-detail-grade">
      <span className="text-xs text-text-muted">{title}</span>
      <b style={{ color }} {...attr}>{value}</b>
      <span className="text-xs text-text-secondary">{sub}</span>
    </div>
  );
  const listForm = !loggedIn ? (
    <p className="m-0 py-2 text-center text-sm text-text-muted">{tr("rivens.detail.loginToList")}</p>
  ) : (
    <div className="flex flex-col gap-4">
      <div className="riven-list-form">
        <Field label={tr("rivens.detail.typeLabel")}>
          <Segmented label={tr("rivens.detail.typeLabel")} active={type} onSelect={setType} items={[{ key: "direct", label: tr("rivens.detail.directSale") }, { key: "auction", label: tr("common.auction") }]} />
        </Field>
        <Field label={tr("rivens.detail.visibilityLabel")}>
          <Segmented label={tr("rivens.detail.visibilityLabel")} active={visibility} onSelect={setVisibility} items={[{ key: "public", label: tr("rivens.detail.public") }, { key: "private", label: tr("rivens.detail.private") }]} />
        </Field>
        <Field label={tr("rivens.detail.descriptionOptional")} className="min-w-40 flex-1">
          <Input type="text" value={description} onChange={(e) => setDescription(e.target.value)} />
        </Field>
      </div>
      <div className="riven-list-form">
        {!isContract && (
          <Field label={tr("common.rank")}>
            <Select
              data-riven-listing-rank
              value={rank}
              options={Array.from({ length: riven.maxRank + 1 }, (_, r) => [r, String(r)] as const)}
              onChange={(e) => setRank(Number(e.target.value))}
            />
          </Field>
        )}
        <Field label={type === "auction" ? tr("rivens.detail.startingBidLabel") : tr("rivens.detail.sellingPriceLabel")}>
          {priceField(price, setPrice, type === "auction" ? tr("rivens.detail.startingBidAria") : tr("rivens.detail.sellingPriceAria"), 1)}
        </Field>
        {type === "auction" && (
          <>
            <Field label={tr("rivens.detail.buyoutOptional")}>{priceField(buyout, setBuyout, tr("rivens.detail.buyoutPriceAria"), 0)}</Field>
            <Field label={tr("rivens.detail.minReputationLabel")}>
              <Input type="number" value={minRep} min={0} aria-label={tr("rivens.detail.minReputationAria")} onChange={(e) => setMinRep(num(e.target.value))} />
            </Field>
          </>
        )}
      </div>
      {canListAtMax && <Checkbox title={tr("rivens.detail.maxRankTooltip", { current: riven.currentRank })} label={tr("rivens.detail.listWithRankStats", { max: riven.maxRank })} checked={rank === riven.maxRank} onChange={(e) => setRank(e.target.checked ? riven.maxRank : riven.currentRank)} />}
      {error && <p className="m-0 text-xs text-danger" data-riven-listing-error>{error}</p>}
      {success && <p className="m-0 text-xs text-success">{success}</p>}
    </div>
  );

  return (
    <Modal
      title={riven.rivenName || riven.weaponName}
      sub={tr("rivens.detail.rivenDetailsAria", { name: riven.weaponName })}
      tr={tr}
      onClose={onClose}
      width="56rem"
      footer={
        loggedIn && (
          <Button variant="primary" onClick={submit} data-riven-listing-submit loading={busy} disabled={busy}>
            {busy ? (isContract ? tr("common.saving") : tr("rivens.detail.listing")) : isContract ? tr("rivens.detail.editContract") : tr("rivens.detail.listOnWfmButton")}
          </Button>
        )
      }
    >
      <div data-riven-detail className="flex flex-col">
        <div className="riven-detail-top">
          <Badge tone="accent">{isContract ? (contract?.isDirectSell ? tr("rivens.detail.directSale") : tr("common.auction")) : typeKey ? tr(typeKey) : riven.rivenType}</Badge>
          {vaulted !== undefined && <Badge tone={vaulted ? "warning" : "success"}>{vaulted ? tr("common.vaulted") : tr("common.unvaulted")}</Badge>}
          {!isContract && (
            <span title={tr("rivens.detail.dispositionTitle", { value: riven.disposition.toFixed(3) })}>
              {dispoStars(riven.disposition)} {riven.disposition.toFixed(2)}
            </span>
          )}
          <span>{tr("rivens.detail.rerolls", { count: riven.rerolls })}</span>
          <span data-riven-detail-rank={riven.currentRank}>{tr("rivens.detail.rank", { current: riven.currentRank, max: riven.maxRank })}</span>
          {riven.masteryReq > 0 && <span>{tr("rivens.mr", { level: riven.masteryReq })}</span>}
          {dissolveEndo !== null && !isContract && (
            <span title={tr("rivens.dissolveValue", { endo: dissolveEndo })} data-riven-dissolve-endo={dissolveEndo}>
              <Readout currency="endo" value={dissolveEndo} chip />
            </span>
          )}
        </div>

        {(hasRoll || hasAttr) && (
          <div className="riven-detail-grades mt-4">
            {hasRoll && gradeCell(tr("rivens.detail.rollQuality"), riven.overallGrade, gradeColor(riven.overallGrade), tr("rivens.detail.percentPerfect", { pct: Math.round(riven.statPerfectness * 100) }), { "data-riven-detail-grade": riven.overallGrade })}
            {hasAttr &&
              gradeCell(
                tr("common.attributes"),
                attrLabel,
                attrGradeColor(riven.attributeGrade),
                <>
                  {tr(positives.length !== 1 ? "rivens.detail.buffsCount" : "rivens.detail.buffCount", { count: positives.length })}
                  {riven.stats.some((s) => !s.positive) && `, ${tr("rivens.detail.oneCurse")}`}
                </>,
                { "data-riven-detail-attr-grade": riven.attributeGrade },
              )}
          </div>
        )}

        <section className="riven-detail-section">
          <header>
            <h3>{tr("common.attributes")}</h3>
          </header>
          <div className="flex flex-col gap-2">
            {riven.stats.map((stat) => (
              <div key={stat.tag} className={`riven-detail-stat ${stat.positive ? "riven-detail-stat--pos" : "riven-detail-stat--neg"}`}>
                <b className="shrink-0 text-base">{stat.multiplier ? `x${stat.displayValue}` : `${num(stat.displayValue) >= 0 ? "+" : ""}${stat.displayValue}%`}</b>
                <span className="min-w-0 flex-1 truncate">{stat.name}</span>
                {stat.grade && (
                  <>
                    <div className="riven-progress" aria-hidden="true">
                      <i style={{ width: `${Math.min((stat.positive ? num(stat.rollFloat) : 1 - num(stat.rollFloat)) * 100, 100)}%`, background: stat.positive ? "var(--success)" : "var(--danger)" }} />
                    </div>
                    <b className="min-w-6 text-center" style={{ color: gradeColor(stat.grade) }} data-riven-stat-grade={stat.grade}>
                      {stat.grade}
                    </b>
                  </>
                )}
              </div>
            ))}
          </div>
        </section>

        <section className="riven-detail-section">
          <header>
            <h3>{tr("rivens.detail.bestAttributesFor", { weapon: riven.weaponName })}</h3>
            <span className="text-xs text-text-muted" data-riven-dictionary-age>{ageLabel(rolls?.updatedAt ?? null, tr)}</span>
            <Button size="sm" icon={<RefreshCw />} loading={refreshing} disabled={refreshing} title={tr("rivens.detail.refreshDictionary")} aria-label={tr("rivens.detail.refreshDictionary")} onClick={() => void refreshRolls()} data-riven-refresh-rolls>
              {tr("common.refresh")}
            </Button>
          </header>
          {bestAttrs ? (
            <div className="riven-best">
              {(
                [
                  ["positives", "rivens.detail.desiredPositives", "text-success"],
                  ["negatives", "rivens.detail.desiredNegatives", "text-danger"],
                ] as const
              ).map(([side, key, tone]) => (
                <div key={side} className="flex flex-col gap-1">
                  <span className={`mb-1 text-xs font-semibold ${tone}`}>{tr(key)}</span>
                  {bestAttrs[side].map((attr) => {
                    const canon = canonicalRivenStatName(attr);
                    const matched = side === "positives" ? myPositive.has(canon) : riven.stats.some((s) => !s.positive && canonicalRivenStatName(s.name) === canon);
                    return (
                      <span key={attr} className={`riven-best-item ${matched ? "is-match" : ""}`}>
                        {attr}
                      </span>
                    );
                  })}
                </div>
              ))}
            </div>
          ) : (
            <p className="m-0 text-xs text-text-muted">{tr("rivens.detail.noGoodRollData")}</p>
          )}
        </section>

        <section className="riven-detail-section">
          <header>
            <h3>{tr("rivens.detail.similarOnWfm")}</h3>
            <Chip pressed={showAuctions} data-similar-auctions-toggle title={tr("rivens.detail.auctionsChipTitle")} onClick={() => onSimilarAuctions(!showAuctions)}>
              {tr("rivens.detail.auctionsChip")}
            </Chip>
          </header>
          {loadingListings ? (
            <p className="m-0 py-3 text-center text-sm text-text-muted">{tr("rivens.detail.searchingAuctions")}</p>
          ) : shown.length === 0 ? (
            <p className="m-0 py-3 text-center text-sm text-text-muted" data-similar-empty>
              {tr("rivens.detail.noSimilarFound")}
            </p>
          ) : (
            <>
              <div className="riven-similar">
                {visible.map(({ listing, pct, matchedNames }) => (
                  <div key={listing.id} className="riven-similar-item" data-similar-listing={listing.id}>
                    <div className="flex items-center gap-2">
                      <Badge tone={pct >= 75 ? "success" : pct >= 40 ? "warning" : "danger"}>{pct}%</Badge>
                      <Readout currency="platinum" value={askedPrice(listing)} />
                      <span className="ml-auto text-text-muted">{tr("rivens.detail.rerolls", { count: listing.rerolls ?? 0 })}</span>
                    </div>
                    <ul className="riven-stats">
                      {listing.stats.map((s) => (
                        <li key={s.urlName} className={`riven-stat ${s.positive === false ? "riven-stat--neg" : "riven-stat--pos"} ${matchedNames.has(s.label.toLowerCase()) ? "" : "line-through opacity-50"}`}>
                          <b>{formatRivenListingStatValue({ name: s.label, value: num(s.value) })}</b>
                          <span>{s.label}</span>
                        </li>
                      ))}
                    </ul>
                    <div className="flex items-center justify-between">
                      <span className="text-text-muted">{listing.seller ?? ""}</span>
                      <a href={`https://warframe.market/auction/${listing.id}`} target="_blank" rel="noreferrer noopener" title={tr("common.openOnWarframeMarket")}>
                        {tr("rivens.detail.wfmLink")}
                      </a>
                    </div>
                  </div>
                ))}
              </div>
              {shown.length > DEFAULT_LISTING_COUNT && (
                <Button size="sm" className="self-center" onClick={() => setShowAll((v) => !v)}>
                  {showAll ? tr("common.showFewer") : tr("rivens.detail.showAll", { count: shown.length })}
                  {!showAll && shown.length > visible.length && <span className="ml-1 text-text-muted">{tr("rivens.detail.moreCount", { count: shown.length - visible.length })}</span>}
                </Button>
              )}
            </>
          )}
        </section>

        <section className="riven-detail-section">
          <header>
            <h3>{isContract ? tr("rivens.detail.wfmContractTitle") : tr("rivens.detail.listOnWfmTitle")}</h3>
          </header>
          {listForm}
        </section>
      </div>
    </Modal>
  );
}
