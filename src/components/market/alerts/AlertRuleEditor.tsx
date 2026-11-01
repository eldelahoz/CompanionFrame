import { X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { Translator } from "../../../lib/i18n";
import { marketAlertsSet, wfmRivenWeapons, type MarketAlertEntry, type MarketAlertRule, type RivenWeapon } from "../../../lib/ipc";
import { attributeLabel, emptyRule, MAX_COOLDOWN_MINUTES, MIN_COOLDOWN_MINUTES, normalizeRule, parseOptionalNumber, RIVEN_ATTRIBUTES, validateRule } from "../../../lib/marketAlerts/ruleForm";
import type { CatalogIndex } from "../../../lib/wfm/catalog";
import { Button, Checkbox, Chip, Field, IconButton, Input, Modal, SearchInput, Segmented, Select, Switch } from "../../ui";

const STATUSES = ["ingame", "online"] as const;
const POLARITIES = ["madurai", "naramon", "vazarin"] as const;

interface Props {
  /** Null starts a new rule of the chosen kind. */
  entry: MarketAlertEntry | null;
  catalog: CatalogIndex | null;
  onClose: () => void;
  onSaved: () => void;
  tr: Translator;
}

const Num = ({ label, value, onChange, step }: { label: string; value: number | null; onChange: (v: number | null) => void; step?: number }) => (
  <Field label={label}>
    <Input type="number" min={0} step={step} value={value ?? ""} onChange={(e) => onChange(parseOptionalNumber(e.target.value))} />
  </Field>
);

const toggled = <T extends string>(list: readonly T[], v: T): T[] => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);

const Section = ({ title }: { title: string }) => <h4 className="m-0 mt-2 border-t border-border pt-4 text-sm font-semibold">{title}</h4>;

function AttributeChips({ label, value, onChange }: { label: string; value: string[]; onChange: (v: string[]) => void }) {
  return (
    <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
      <legend className="mb-1 p-0 text-xs text-text-muted">{label}</legend>
      <div className="flex flex-wrap gap-1">
        {RIVEN_ATTRIBUTES.map((a) => (
          <Chip key={a} pressed={value.includes(a)} onClick={() => onChange(toggled(value, a))}>
            {attributeLabel(a)}
          </Chip>
        ))}
      </div>
    </fieldset>
  );
}

function StatusPicker({ label, value, onChange, tr }: { label: string; value: ("ingame" | "online")[]; onChange: (v: ("ingame" | "online")[]) => void; tr: Translator }) {
  return (
    <div className="flex flex-wrap items-center gap-4 text-sm text-text-secondary">
      {label}
      {STATUSES.map((s) => (
        <Checkbox key={s} label={tr(s === "ingame" ? "common.inGame" : "common.online")} checked={value.includes(s)} onChange={() => onChange(toggled(value, s))} />
      ))}
    </div>
  );
}

export default function AlertRuleEditor({ entry, catalog, onClose, onSaved, tr }: Props) {
  const [rule, setRule] = useState<MarketAlertRule>(() => entry?.rule ?? emptyRule("item"));
  const [native, setNative] = useState(entry?.binding.native ?? true);
  const [query, setQuery] = useState("");
  const [weapons, setWeapons] = useState<RivenWeapon[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const matches = useMemo(() => (catalog && query.trim() ? catalog.search(query, 6) : []), [catalog, query]);
  useEffect(() => {
    if (rule.kind === "riven" && weapons.length === 0) wfmRivenWeapons().then(setWeapons, () => {});
  }, [rule.kind, weapons.length]);

  const patch = (p: Partial<MarketAlertRule>) => setRule((r) => ({ ...r, ...p }));
  const item = rule.item!;
  const riven = rule.riven!;
  const setItem = (p: Partial<typeof item>) => patch({ item: { ...item, ...p } });
  const setRiven = (p: Partial<typeof riven>) => patch({ riven: { ...riven, ...p } });

  const submit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    const problem = validateRule(rule);
    if (problem) return setError(tr(problem));
    setBusy(true);
    try {
      await marketAlertsSet({ rule: normalizeRule(rule), binding: { native } });
      onSaved();
    } catch (err) {
      setError(tr("marketAlerts.saveFailed", { error: (err as Error).message }));
      setBusy(false);
    }
  };

  return (
    <Modal
      title={tr(entry ? "marketAlerts.editRule" : "marketAlerts.newRule")}
      tr={tr}
      onClose={onClose}
      width="36rem"
      footer={
        <>
          <Button onClick={onClose}>{tr("common.cancel")}</Button>
          <Button variant="primary" loading={busy} disabled={busy} onClick={() => void submit()}>
            {tr("common.save")}
          </Button>
        </>
      }
    >
      <form data-alert-editor className="flex flex-col gap-4" onSubmit={submit}>
        <Field label={tr("marketAlerts.ruleName")}>
          <Input value={rule.name} maxLength={60} onChange={(e) => patch({ name: e.target.value })} autoFocus />
        </Field>
        {!entry && (
          <Segmented
            label={tr("common.type")}
            active={rule.kind as "item" | "riven"}
            onSelect={(k) => setRule({ ...emptyRule(k), name: rule.name })}
            items={[
              { key: "item", label: tr("common.item") },
              { key: "riven", label: tr("common.rivens") },
            ]}
          />
        )}

        <Section title={tr("marketAlerts.section.watch")} />
        {rule.kind === "item" ? (
          <>
            {item.itemUrlName ? (
              <div className="flex items-center justify-between gap-2" data-alert-item>
                <strong>{catalog?.bySlug?.(item.itemUrlName)?.name ?? item.itemUrlName}</strong>
                <IconButton label={tr("orderModal.clearItem")} onClick={() => setItem({ itemUrlName: "" })}>
                  <X />
                </IconButton>
              </div>
            ) : (
              <div className="flex flex-col gap-1">
                <SearchInput label={tr("orderModal.searchItemsPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} placeholder={tr("orderModal.searchItemsPlaceholder")} />
                {matches.map((m) => (
                  <Button key={m.id} variant="ghost" size="sm" className="justify-start" onClick={() => (setItem({ itemUrlName: m.slug }), setQuery(""))}>
                    {m.name}
                  </Button>
                ))}
              </div>
            )}
            <Segmented
              label={tr("common.type")}
              active={item.side}
              onSelect={(s) => setItem({ side: s })}
              items={[
                { key: "sell", label: tr("orderModal.sell") },
                { key: "buy", label: tr("orderModal.buy") },
              ]}
            />
            <div className="grid grid-cols-2 gap-3">
              <Num label={tr("common.price") + " ≤"} value={item.maxPlatinum} onChange={(v) => setItem({ maxPlatinum: v })} />
              <Num label={tr("common.price") + " ≥"} value={item.minPlatinum} onChange={(v) => setItem({ minPlatinum: v })} />
              <Num label={tr("marketAlerts.minQuantity")} value={item.minQuantity} onChange={(v) => setItem({ minQuantity: v })} />
              <span />
              <Num label={tr("marketAlerts.ownedBelow")} value={item.ownedBelow} onChange={(v) => setItem({ ownedBelow: v })} />
              <Num label={tr("marketAlerts.ownedAbove")} value={item.ownedAbove} onChange={(v) => setItem({ ownedAbove: v })} />
            </div>
            <StatusPicker label={tr("marketAlerts.sellerStatus")} value={item.statuses} onChange={(v) => setItem({ statuses: v })} tr={tr} />
          </>
        ) : (
          <>
            {/* shortcut: stat bounds, mastery/mod/reroll ranges and the god-roll helper are not editable here; a rule that carries them keeps them */}
            <Field label={tr("marketAlerts.weaponRequired")}>
              <Input list="alert-weapons" value={riven.weaponUrlName} onChange={(e) => setRiven({ weaponUrlName: e.target.value.trim().toLowerCase() })} />
            </Field>
            <datalist id="alert-weapons">
              {weapons.map((w) => (
                <option key={w.id} value={w.slug}>
                  {w.name}
                </option>
              ))}
            </datalist>
            <AttributeChips label={tr("marketAlerts.requiredPositive")} value={riven.requirePositive} onChange={(v) => setRiven({ requirePositive: v })} />
            <AttributeChips label={tr("marketAlerts.excludedStats")} value={riven.excludeAttributes} onChange={(v) => setRiven({ excludeAttributes: v })} />
            <Segmented
              label={tr("marketAlerts.negativeRequired")}
              active={String(riven.hasNegative)}
              onSelect={(k) => setRiven({ hasNegative: k === "null" ? null : k === "true" })}
              items={[
                { key: "null", label: tr("common.all") },
                { key: "true", label: tr("marketAlerts.negativeRequired") },
                { key: "false", label: tr("marketAlerts.negativeForbidden") },
              ]}
            />
            <div className="grid grid-cols-2 gap-3">
              <Num label={tr("common.price") + " ≤"} value={riven.maxPlatinum} onChange={(v) => setRiven({ maxPlatinum: v })} />
              <Num label={tr("common.price") + " ≥"} value={riven.minPlatinum} onChange={(v) => setRiven({ minPlatinum: v })} />
              <Num label={tr("marketAlerts.minEndoPerPlat")} value={riven.minEndoPerPlat} step={0.1} onChange={(v) => setRiven({ minEndoPerPlat: v })} />
              <Field label={tr("marketAlerts.polarity")}>
                <Select
                  value={riven.polarity ?? ""}
                  options={[["", tr("common.all")] as const, ...POLARITIES.map((p) => [p, p] as const)]}
                  onChange={(e) => setRiven({ polarity: (e.target.value || null) as typeof riven.polarity })}
                />
              </Field>
            </div>
            <Checkbox label={tr("marketAlerts.includeBidOnly")} checked={riven.includeBidOnly === true} onChange={(e) => setRiven({ includeBidOnly: e.target.checked || null })} />
            <StatusPicker label={tr("marketAlerts.sellerStatus")} value={riven.statuses} onChange={(v) => setRiven({ statuses: v })} tr={tr} />
          </>
        )}

        <Section title={tr("marketAlerts.section.delivery")} />
        <Field label={tr("marketAlerts.cooldownMinutes")}>
          <Input type="number" min={MIN_COOLDOWN_MINUTES} max={MAX_COOLDOWN_MINUTES} disabled={rule.noCooldown} value={rule.cooldownMinutes} onChange={(e) => patch({ cooldownMinutes: Number(e.target.value) })} />
          <span className="text-xs text-text-muted">{tr("marketAlerts.cooldownHint", { min: MIN_COOLDOWN_MINUTES, max: MAX_COOLDOWN_MINUTES })}</span>
        </Field>
        <Checkbox label={tr("marketAlerts.noCooldown")} title={tr("marketAlerts.noCooldownHint")} checked={rule.noCooldown} onChange={(e) => patch({ noCooldown: e.target.checked })} />
        <Switch label={tr("marketAlerts.desktopNotification")} checked={native} onChange={(e) => setNative(e.target.checked)} />
        <Switch label={tr("marketAlerts.enabled")} checked={rule.enabled} onChange={(e) => patch({ enabled: e.target.checked })} />
        {error && (
          <p className="m-0 text-sm text-danger" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
