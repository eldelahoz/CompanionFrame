import { useState } from "react";
import { Button, Input, Select } from "../ui";
import type { Translator } from "../../lib/i18n";
import type { WorldFissureAlert } from "../../lib/ipc";
import { useWorldNotify } from "../../stores/worldNotify";

const TIERS = ["Lith", "Meso", "Neo", "Axi", "Requiem", "Omnia"];
// Labels the parser emits; a rule matches by label.
const MISSIONS = ["Capture", "Defense", "Disruption", "Excavation", "Exterminate", "Hive", "Interception", "Mobile Defense", "Rescue", "Sabotage", "Spy", "Survival", "Void Cascade", "Void Flood"];
const STEEL = [
  ["any", "settings.fissureAnyMode"],
  ["normal", "common.normal"],
  ["steel", "common.steelPath"],
] as const;

const blank = (): WorldFissureAlert => ({ id: "", tier: "any", missionType: "any", steelPath: "any", planet: "" });

/** Fissure alert rules and the cycle heads-up lead time (the per-cycle bells live in WorldView). */
export default function WorldNotifySettings({ tr }: { tr: Translator }) {
  const { prefs, save } = useWorldNotify();
  const [draft, setDraft] = useState<WorldFissureAlert>(blank);
  if (!prefs) return null;
  const rules = prefs.fissureAlerts;
  const add = () => {
    void save({ ...prefs, fissureAlerts: [...rules, { ...draft, id: `${Date.now()}-${rules.length}`, planet: draft.planet.trim() }] });
    setDraft(blank());
  };
  const describe = (r: WorldFissureAlert) =>
    [
      r.tier === "any" ? tr("settings.fissureAnyTier") : r.tier,
      r.missionType === "any" ? tr("settings.fissureAnyMission") : r.missionType,
      tr(STEEL.find(([v]) => v === r.steelPath)![1]),
      r.planet || tr("settings.fissureAnyPlanet"),
    ].join(", ");

  return (
    <section className="flex flex-col gap-2" data-world-notify>
      <h3 className="m-0 text-sm font-semibold text-text-primary">{tr("settings.fissureAlertsTitle")}</h3>
      <p className="m-0 text-sm text-text-secondary">{tr("settings.fissureAlertsDesc")}</p>
      <label className="flex items-center justify-between gap-4">
        <span>{tr("world.notifyBeforeCycleChange")}</span>
        <Input
          type="number"
          min={0}
          max={120}
          className="!w-20"
          value={prefs.cycleAlertMinutesBefore}
          onChange={(e) => void save({ ...prefs, cycleAlertMinutesBefore: Math.max(0, Math.min(120, Number(e.target.value) || 0)) })}
        />
      </label>
      {rules.length === 0 && <span className="text-sm text-text-muted">{tr("settings.fissureNoRules")}</span>}
      {rules.map((r) => (
        <div key={r.id} className="flex items-center justify-between gap-2 text-sm">
          <span>{describe(r)}</span>
          <Button size="sm" variant="danger" onClick={() => void save({ ...prefs, fissureAlerts: rules.filter((x) => x.id !== r.id) })}>
            {tr("settings.fissureRemoveAlert")}
          </Button>
        </div>
      ))}
      <div className="flex flex-wrap gap-2">
        <Select aria-label={tr("settings.fissureAnyTier")} value={draft.tier} onChange={(e) => setDraft({ ...draft, tier: e.target.value })} options={[["any", tr("settings.fissureAnyTier")], ...TIERS.map((t) => [t, t] as const)]} />
        <Select aria-label={tr("settings.fissureAnyMission")} value={draft.missionType} onChange={(e) => setDraft({ ...draft, missionType: e.target.value })} options={[["any", tr("settings.fissureAnyMission")], ...MISSIONS.map((m) => [m, m] as const)]} />
        <Select aria-label={tr("settings.fissureAnyMode")} value={draft.steelPath} onChange={(e) => setDraft({ ...draft, steelPath: e.target.value as WorldFissureAlert["steelPath"] })} options={STEEL.map(([v, k]) => [v, tr(k)] as const)} />
        <Input className="!w-36" aria-label={tr("settings.fissureAnyPlanet")} placeholder={tr("settings.fissureAnyPlanet")} value={draft.planet} onChange={(e) => setDraft({ ...draft, planet: e.target.value })} />
        <Button size="sm" onClick={add}>
          {tr("settings.fissureAdd")}
        </Button>
      </div>
    </section>
  );
}
