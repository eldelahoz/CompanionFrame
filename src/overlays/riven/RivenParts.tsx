import { convertFileSrc } from "@tauri-apps/api/core";
import { ArrowDown, ArrowUp, Dices, Equal, Eye, ExternalLink, TriangleAlert } from "lucide-react";
import ItemImage from "../../components/ItemImage";
import { toMarketSlug } from "../../lib/marketNaming";
import type { Translator } from "../../lib/i18n";
import { itemsByName } from "../../lib/world/itemsByName";
import { OvReadout } from "../common/OvReadout";
import type { RivenCardView_Serialize as Card, RivenListing_Serialize as Listing, RivenOverlayView_Serialize as View, RivenOverlayStat_Serialize as Stat, RivenVerdict } from "../../bindings";
import { statAbbr } from "./statAbbr";

type Tr = { tr: Translator };
type Art = ReturnType<typeof itemsByName>;

/** "A+" -> "a": the app's grade colour scale keys on the letter. */
const gradeTone = (g: string) => g.trim().charAt(0).toLowerCase();

// Stats the card shows without a % sign (metres, seconds, counts).
const PLAIN = new Set(["Punch Through", "Range", "Combo Duration", "Initial Combo"]);
const fmt = (v: number | null | undefined, s: Stat) => {
  if (v == null) return "?";
  // The card's precision: two decimals for multipliers, one otherwise (the range edges arrive unrounded).
  if (s.multiplier) return `x${+v.toFixed(2)}`;
  return `${(s.printedPositive ?? s.positive) ? "+" : "−"}${+v.toFixed(1)}${PLAIN.has(s.name) ? "" : "%"}`;
};

// The item DB's public art by weapon name; the wfmimg set art (warframe.market lists prime weapons as sets) is the last fallback.
const setArt = (weapon: string | null | undefined) =>
  weapon && "__TAURI_INTERNALS__" in window ? convertFileSrc(`${toMarketSlug(weapon)}_set`, "wfmimg") : null;

const Grade = ({ g, xl, title }: { g: string; xl?: boolean; title?: string }) => (
  <span className={`ov-grade ${xl ? "ov-grade-xl " : ""}riven-grade`} data-grade={gradeTone(g)} title={title}>
    {g}
  </span>
);

export function CardHead({ card, art, tr }: Tr & { card: Card; art: Art }) {
  const weapon = card.weapon;
  const sub = [card.mr != null && tr("overlay.riven.mr", { value: card.mr }), card.cardRolls != null && tr("overlay.riven.rolls", { count: card.cardRolls })].filter(Boolean).join(", ");
  return (
    <div className="rv-head">
      <span className="rv-wpn">
        <ItemImage slug={weapon ? toMarketSlug(weapon) : null} src={weapon ? art.get(weapon)?.imageUrl : null} fallback={setArt(weapon)} cls="ov-art" tr={tr} />
      </span>
      <div>
        <h3>{weapon ?? "Riven"}</h3>
        {sub && <div className="rv-sub">{sub}</div>}
      </div>
      {card.overallGrade && <Grade g={card.overallGrade} xl title={tr("overlay.riven.overallGrade")} />}
    </div>
  );
}

export function CardStats({ card, tr, best }: Tr & { card: Card; best?: boolean }) {
  return (
    <>
      <ul className="rv-stats">
        {card.stats.map((s, i) => {
          const range = s.min != null && s.max != null && s.quality != null;
          return (
            <li key={i} className={s.positive ? "" : "riven-neg"}>
              <span className="rv-v">{fmt(s.value, s)}</span>
              <span className="rv-n">{s.name}</span>
              {s.grade ? <Grade g={s.grade} /> : <span />}
              {range && (
                <>
                  <span className="rv-q" style={{ "--q": `${s.quality}%` } as React.CSSProperties} data-grade={s.grade ? gradeTone(s.grade) : undefined} title={tr("overlay.riven.rangeTitle", { min: fmt(s.min, s), max: fmt(s.max, s) })}>
                    <i />
                  </span>
                  <span className="rv-qh">
                    <span>{fmt(s.min, s)}</span>
                    <span>{tr("overlay.riven.percentOfRange", { pct: s.quality ?? 0 })}</span>
                    <span>{fmt(s.max, s)}</span>
                  </span>
                </>
              )}
            </li>
          );
        })}
      </ul>
      {!card.weapon && <p className="riven-note">{tr("overlay.riven.weaponMissing")}</p>}
      {card.unsure && <p className="riven-note">{tr("overlay.riven.textTooSmall")}</p>}
      {best && card.best && (
        <p className="riven-note rv-best">
          <span>{`${tr("overlay.riven.bestPositives")}: ${card.best.positives.join(", ")}`}</span>
          {card.best.negatives.length > 0 && <span>{`${tr("overlay.riven.bestNegatives")}: ${card.best.negatives.join(", ")}`}</span>}
        </p>
      )}
    </>
  );
}

const matchTone = (p: number) => (p >= 75 ? "hi" : p >= 40 ? "md" : "lo");
const MatchChip = ({ pct }: { pct: number }) => (
  <span className="rv-m" data-tone={matchTone(pct)}>
    {pct}%
  </span>
);

/** The Est. value block: median, the p25-p75 bar and the sample size; skeleton until the market answers. */
export function Estimate({ card, tr }: Tr & { card: Card }) {
  const a = card.analysis;
  if (!a) {
    return (
      <div className="rv-est" aria-busy="true">
        <span className="rv-sk" style={{ width: "100%", height: "3.4em" }} />
      </div>
    );
  }
  const e = a.estimate;
  if (!e) {
    return (
      <div className="rv-est">
        <div className="rv-est-row">
          <span className="rv-lab">{tr("overlay.riven.estValue")}</span>
          <span className="rv-faint">{tr("overlay.riven.noListings")}</span>
        </div>
      </div>
    );
  }
  const fallback = e.source === "weaponMedian";
  const max = Math.max(1, Math.round(e.p75 * 1.15));
  const pos = (v: number) => Math.min(100, (v / max) * 100);
  return (
    <div className="rv-est">
      <div className="rv-est-row">
        <span className="rv-lab">
          {tr("overlay.riven.estValue")} {e.lowConfidence && <span className="rv-low">{tr("overlay.riven.lowConfidence")}</span>}
        </span>
        <OvReadout currency="platinum" value={fallback ? `~${e.median}` : e.median} size="lg" />
      </div>
      {fallback ? (
        <div className="rv-dashed" />
      ) : (
        <div className="rv-range" role="img" aria-label={`${e.p25} - ${e.p75}`}>
          <span className="rv-span" style={{ left: `${pos(e.p25)}%`, right: `${100 - pos(e.p75)}%` }} />
          <span className="rv-mark" style={{ left: `${pos(e.median)}%` }} />
        </div>
      )}
      <div className="rv-legend">
        {fallback ? (
          <span>{tr("overlay.riven.medianLabel", { weapon: card.weapon ?? "" })}</span>
        ) : (
          <>
            <span>{e.p25}</span>
            <span>{tr("overlay.riven.listingsWeighted", { count: e.n })}</span>
            <span>{e.p75}</span>
          </>
        )}
      </div>
      {e.best && (
        <div className="rv-best-match">
          <MatchChip pct={e.best.matchPct} />
          {tr("overlay.riven.bestMatch")} <OvReadout currency="platinum" value={e.best.platinum} />
        </div>
      )}
    </div>
  );
}

const STATUS = { ingame: "statusIngame", online: "statusOnline", offline: "statusOffline" } as const;

function ListingRow({ l, tr }: Tr & { l: Listing }) {
  const status = l.sellerStatus && l.sellerStatus in STATUS ? (l.sellerStatus as keyof typeof STATUS) : null;
  return (
    <a className="rv-li" href={l.url} target="_blank" rel="noreferrer" title={tr("overlay.riven.openListing")}>
      <MatchChip pct={l.matchPct} />
      <OvReadout currency="platinum" value={l.platinum} />
      <span className="rv-ab">
        {l.stats.map((s, i) => (
          <em key={i} data-hit={s.matched || undefined} data-neg={!s.positive || undefined}>
            {statAbbr(s.key, s.positive)}
          </em>
        ))}
      </span>
      {status ? <span className="rv-st" data-st={status}>{tr(`overlay.riven.${STATUS[status]}`)}</span> : <span />}
      <ExternalLink className="ov-icon rv-go" aria-hidden />
    </a>
  );
}

export function Listings({ card, tr }: Tr & { card: Card }) {
  const list = card.analysis?.listings.filter((l) => l.matchPct > 0) ?? [];
  if (!list.length) return null;
  return (
    <div className="rv-lst">
      <div className="rv-lst-h">
        <span>{tr("overlay.riven.similar")}</span>
        <span>{tr("overlay.riven.sortedByMatch")}</span>
      </div>
      {/* Four rows: five pushed the panel, now under the currency bar, over the game's "fits in" box. */}
      {list.slice(0, 4).map((l, i) => (
        <ListingRow key={i} l={l} tr={tr} />
      ))}
    </div>
  );
}

const marketSearch = (weapon: string | null | undefined) => `https://warframe.market/auctions/search?type=riven${weapon ? `&weapon_url_name=${toMarketSlug(weapon)}` : ""}`;

export function MarketLink({ card, tr }: Tr & { card: Card }) {
  const none = card.analysis?.estimate?.source !== "listings" && card.analysis != null;
  return (
    <a className="rv-open" href={marketSearch(card.weapon)} target="_blank" rel="noreferrer">
      <ExternalLink className="ov-icon" aria-hidden />
      {tr(none ? "overlay.riven.searchMarket" : "overlay.riven.openMarket")}
    </a>
  );
}

export function NoListingsNote({ card, tr }: Tr & { card: Card }) {
  const e = card.analysis?.estimate;
  if (!card.analysis || card.analysis.listings.some((l) => l.matchPct > 0)) return null;
  return (
    <div className="rv-note">
      <TriangleAlert className="ov-icon" aria-hidden />
      <span>
        <b>{tr("overlay.riven.noListings")}</b>{" "}
        {e?.source === "weaponMedian" && tr("overlay.riven.medianNote", { weapon: card.weapon ?? "", count: e.n })}
      </span>
    </div>
  );
}

const VERDICT_ICON = { take: ArrowUp, keep: ArrowDown, close: Equal } as const;
const VERDICT_KEY = { take: "verdictTake", keep: "verdictKeep", close: "verdictClose" } as const;
const GRADE_KEY = { better: "gradeBetter", same: "gradeSame", worse: "gradeWorse", unknown: "gradeUnknown" } as const;
const REASON_KEY = { take: "reasonDelta", keep: "reasonKeep", close: "reasonClose" } as const;

const signed = (n: number) => (n > 0 ? `+${n}` : n < 0 ? `−${Math.abs(n)}` : "0");

export function VerdictBanner({ v, tr }: Tr & { v: RivenVerdict | null }) {
  if (!v) {
    return (
      <div className="rv-verdict" data-v="wait" role="status">
        <Eye className="ov-icon" aria-hidden />
        <div>
          <strong>{tr("overlay.riven.verdictWait")}</strong>
          <span>{tr("overlay.riven.reasonWait")}</span>
        </div>
        <span />
      </div>
    );
  }
  const Icon = VERDICT_ICON[v.kind];
  const delta = v.kind === "close" ? Math.abs(v.delta) : signed(v.delta);
  return (
    <div className="rv-verdict" data-v={v.kind} role="status">
      <Icon className="ov-icon" aria-hidden />
      <div>
        <strong>{tr(`overlay.riven.${VERDICT_KEY[v.kind]}`)}</strong>
        <span>{tr(`overlay.riven.${REASON_KEY[v.kind]}`, { delta, grade: tr(`overlay.riven.${GRADE_KEY[v.gradeCmp]}`) })}</span>
      </div>
      <span className="rv-delta">{v.kind === "close" ? `±${delta}p` : `${delta}p`}</span>
    </div>
  );
}

export function Footer({ view, card, tr }: Tr & { view: View; card: Card | null }) {
  const own = card?.cardRolls ?? null;
  const rolls = own ?? view.session?.rollCount ?? null;
  const kuva = own != null ? card?.cardKuvaSpent : (view.session?.totalKuvaSpent ?? null);
  const mr = card?.mr ?? null;
  if (rolls == null && mr == null && view.nextKuva == null) return null;
  return (
    <div className="rv-foot">
      {mr != null && <span>{tr("overlay.riven.mr", { value: mr })}</span>}
      {rolls != null && (
        <span>
          <Dices className="ov-icon" aria-hidden />
          {tr(own != null ? "overlay.riven.rolls" : "overlay.riven.sessionRolls", { count: rolls })}
        </span>
      )}
      {kuva != null && <span>{tr("overlay.riven.kuvaSpent", { value: kuva.toLocaleString() })}</span>}
      {own != null && view.session && <span>{tr("overlay.riven.sessionTally", { count: view.session.rollCount, value: (view.session.totalKuvaSpent ?? 0).toLocaleString() })}</span>}
      {view.nextKuva != null && <span className="rv-r">{tr("overlay.riven.nextCycle", { value: view.nextKuva.toLocaleString() })}</span>}
    </div>
  );
}

const Sk = ({ w, h }: { w: string; h: string }) => <span className="rv-sk" style={{ width: w, height: h }} />;

/** Skeleton columns and the "Reading cards…" banner while a compare screen is being read. */
export function Loading({ tr }: Tr) {
  return (
    <div className="overlay-panel riven-card rv-cmp" aria-busy="true">
      <div className="rv-verdict" data-v="wait" role="status">
        <span className="rv-spin" />
        <div className="rv-reading">{tr("overlay.riven.readingCards")}</div>
        <span />
      </div>
      <div className="rv-cols">
        {[false, true].map((isNew) => (
          <div key={String(isNew)} className="rv-col" style={{ gap: ".8em" }}>
            <div className="rv-col-lab">
              <span>{tr(isNew ? "overlay.riven.newRoll" : "overlay.riven.current")}</span>
            </div>
            <div className="rv-head">
              <Sk w="3.4em" h="3.4em" />
              <div style={{ display: "grid", gap: ".4em" }}>
                <Sk w="80%" h="1em" />
                <Sk w="50%" h=".8em" />
              </div>
              <Sk w="2.3em" h="2.3em" />
            </div>
            {[0, 1, 2].map((i) => (
              <Sk key={i} w="100%" h="1.3em" />
            ))}
            <Sk w="100%" h="5em" />
          </div>
        ))}
      </div>
    </div>
  );
}
