import type { Invasion, InvasionSide } from "../../bindings";
import type { Translator } from "../../lib/i18n";

const FACTION_TEXT: Record<string, string> = {
  grineer: "text-[var(--world-faction-grineer)]",
  corpus: "text-[var(--world-faction-corpus)]",
  infested: "text-[var(--world-faction-infested)]",
};
const FACTION_BG: Record<string, string> = {
  grineer: "bg-[var(--world-faction-grineer)]",
  corpus: "bg-[var(--world-faction-corpus)]",
  infested: "bg-[var(--world-faction-infested)]",
};

function rewardLabel(tr: Translator, locale: string, side: InvasionSide): string {
  const r = side.reward;
  if (r.countedItems.length > 0) return r.countedItems.map((ci) => (ci.count > 1 ? `${ci.count}x ${ci.type}` : ci.type)).join(", ");
  if (r.items.length > 0) return r.items.join(", ");
  if (r.credits > 0) return tr("world.creditsAmount", { amount: r.credits.toLocaleString(locale) });
  return "";
}

export default function InvasionItem({ inv, tr, locale }: { inv: Invasion; tr: Translator; locale: string }) {
  const atk = inv.attacker.faction.toLowerCase();
  const def = inv.defender.faction.toLowerCase();
  const completion = inv.completion ?? 0;
  const attackerPct = Math.max(0, Math.min(100, completion));
  const defenderPct = Math.max(0, Math.min(100, 100 - completion));
  const factionLabel = "shrink-0 text-xs font-semibold";
  return (
    <div className="flex flex-col gap-1 border-b border-border py-2 last:border-b-0" data-invasion={inv.id}>
      <span className="text-sm font-semibold text-text-primary">{inv.node}</span>
      <div className="flex items-center gap-2 text-sm">
        <span className={`${factionLabel} ${FACTION_TEXT[atk] ?? ""}`}>{inv.attacker.faction}</span>
        <span className="min-w-0 flex-1 truncate text-right text-text-primary">{rewardLabel(tr, locale, inv.attacker)}</span>
        <span className="shrink-0 text-xs text-text-muted">{tr("world.vs")}</span>
        <span className="min-w-0 flex-1 truncate text-left text-text-primary">{rewardLabel(tr, locale, inv.defender)}</span>
        <span className={`${factionLabel} ${FACTION_TEXT[def] ?? ""}`}>{inv.defender.faction}</span>
      </div>
      <div className="flex h-1 overflow-hidden rounded-full">
        <div className={`h-full transition-[width] duration-300 ${FACTION_BG[atk] ?? ""}`} style={{ width: `${attackerPct}%` }} />
        <div className={`h-full transition-[width] duration-300 ${FACTION_BG[def] ?? ""}`} style={{ width: `${defenderPct}%` }} />
      </div>
      <span className="flex items-center gap-1 text-xs tabular-nums text-text-secondary">
        <span className={FACTION_TEXT[atk] ?? ""}>{completion.toFixed(1)}%</span>
        <span className="opacity-40">-</span>
        <span className={FACTION_TEXT[def] ?? ""}>{(100 - completion).toFixed(1)}%</span>
      </span>
    </div>
  );
}
