import { useMemo, useState } from "react";
import type { ProfileAppearanceItem } from "../../bindings";
import type { ItemDatabase, SavedLoadout } from "../../lib/ipc";
import { itemLabel } from "../../lib/itemLabel";
import type { MessageKey, Translator } from "../../lib/i18n";
import { Badge, Card, CardHeader, Chip, EmptyState, Field, Select } from "../ui";

const GROUPS = ["pricol", "attcol", "syancol", "sigcol"] as const;
const CHANNELS = ["t0", "t1", "t2", "t3", "m0", "m1", "en", "e1"] as const;
const CATEGORY_KEYS: Record<string, MessageKey> = {
  Suits: "profile.warframe",
  LongGuns: "profile.primaryWeapon",
  Pistols: "profile.secondaryWeapon",
  Melee: "rivens.type.melee",
};
const GROUP_KEYS: Record<(typeof GROUPS)[number], MessageKey> = {
  pricol: "profile.bodyColors",
  attcol: "profile.attachmentColors",
  syancol: "profile.syandanaColors",
  sigcol: "profile.sigilColors",
};
const COLOR_KEYS: Record<(typeof CHANNELS)[number], MessageKey> = {
  t0: "common.primary",
  t1: "appearance.label.textSecondary",
  t2: "profile.tertiary",
  t3: "pet.trait.accent",
  m0: "profile.emissiveOne",
  m1: "profile.emissiveTwo",
  en: "profile.energyOne",
  e1: "profile.energyTwo",
};
const SUIT_SLOT_KEYS: Record<number, MessageKey> = {
  0: "profile.helmet",
  7: "profile.skin",
  5: "profile.animation",
  6: "profile.syandana",
  8: "profile.chest",
  1: "profile.leftShoulder",
  9: "profile.rightShoulder",
  2: "profile.leftLeg",
  10: "profile.rightLeg",
  12: "profile.frontSigil",
  13: "profile.backSigil",
  16: "profile.ephemera",
  25: "profile.signa",
};
const WEAPON_SLOT_KEYS: Record<number, MessageKey> = { 0: "profile.skin", 2: "profile.holster", 6: "profile.meleeAttachment" };
const PUBLIC = -1;

function Thumb({ src }: { src?: string | null }) {
  return <span className="ui-thumb">{src && <img src={src} alt="" loading="lazy" />}</span>;
}

/** Equipped look of the public profile or of one saved loadout: gear picker, appearance configs, cosmetics, colours and mods. */
export default function ProfileAppearance({ items, loadouts, db, tr }: { items: ProfileAppearanceItem[]; loadouts: SavedLoadout[]; db: ItemDatabase; tr: Translator }) {
  const [loadoutPick, setLoadoutPick] = useState<number | null>(null);
  const [selected, setSelected] = useState(0);
  const [configPick, setConfigPick] = useState<number | null>(null);
  const [modPick, setModPick] = useState<number | null>(null);

  const loadoutIndex = loadoutPick ?? (loadouts.length ? 0 : PUBLIC);
  const loadout = loadouts[loadoutIndex] ?? null;
  const shown = useMemo(() => (loadout ? loadout.slots.flatMap((s) => (s.item ? [s.item] : [])) : items), [loadout, items]);
  const item = shown[Math.min(selected, Math.max(0, shown.length - 1))] ?? null;
  const name = (type: string) => itemLabel(type, db[type]);

  const configIndex = Math.min(configPick ?? item?.activeConfig ?? 0, Math.max(0, (item?.configs.length ?? 0) - 1));
  const config = item?.configs[configIndex] ?? null;
  const colors = GROUPS.filter((g) => Object.keys(config?.colors[g] ?? {}).length > 0);
  // Only saved-loadout items carry mod builds.
  const mods = (loadout && item && loadout.slots.find((s) => s.item === item)?.item) || null;
  const modIndex = modPick ?? mods?.activeModConfig ?? null;
  const modConfig = modIndex === null ? null : (mods?.modConfigs[modIndex] ?? null);
  const slotKey = (slot: number) => (item?.category === "Suits" ? SUIT_SLOT_KEYS : WEAPON_SLOT_KEYS)[slot];
  const pickGear = (i: number) => {
    setSelected(i);
    setConfigPick(null);
    setModPick(null);
  };

  return (
    <div className="flex flex-col gap-4" data-profile-appearance>
      {(loadouts.length > 0 || items.length > 0) && (
        <div className="flex flex-wrap items-end gap-3">
          {loadouts.length > 0 && (
            <Field label={tr("profile.savedLoadouts")}>
              <Select
                data-profile-loadout
                value={loadoutIndex}
                onChange={(e) => {
                  setLoadoutPick(Number(e.target.value));
                  pickGear(0);
                }}
                options={[...loadouts.map((l, i) => [i, l.name || tr("profile.loadoutNumber", { index: i + 1 })] as const), ...(items.length ? [[PUBLIC, tr("profile.publicLoadout")] as const] : [])]}
              />
            </Field>
          )}
          <p className="m-0 pb-1.5 text-sm text-text-secondary">{tr(loadout ? "profile.savedLoadoutsSource" : "profile.appearanceSource")}</p>
        </div>
      )}
      {loadouts.length === 0 && (
        <p className="m-0 text-sm text-text-muted" data-profile-loadouts-unavailable>
          {tr("profile.savedLoadoutsUnavailable")}
        </p>
      )}
      {loadout?.slots
        .filter((s) => !s.item)
        .map((s) => (
          <p key={s.category} className="m-0 text-sm text-text-muted" data-profile-slot-unavailable={s.category}>
            {tr(CATEGORY_KEYS[s.category] ?? "profile.unavailable")}: {tr("profile.unavailable")}
          </p>
        ))}

      {!item ? (
        <Card>
          <EmptyState title={tr("profile.noAppearance")} />
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-4">
            {shown.map((entry, i) => (
              <button key={`${entry.category}-${entry.type}`} type="button" className="ui-card flex items-center gap-3 p-3 text-left" aria-pressed={selected === i} data-profile-appearance-item={entry.category} onClick={() => pickGear(i)}>
                <Thumb src={db[entry.type]?.imageUrl} />
                <span className="flex min-w-0 flex-col">
                  <span className="text-xs text-text-muted">{tr(CATEGORY_KEYS[entry.category] ?? "profile.unavailable")}</span>
                  <span className="truncate font-medium text-text-primary">{name(entry.type)}</span>
                </span>
              </button>
            ))}
          </div>

          <Card>
            <CardHeader
              title={name(item.type)}
              meta={item.hiddenWhenHolstered == null ? undefined : tr(item.hiddenWhenHolstered ? "profile.hiddenWhenHolstered" : "profile.shownWhenHolstered")}
              tools={item.configs.map((look, i) => (
                <Chip key={i} pressed={configIndex === i} data-profile-config={i} onClick={() => setConfigPick(i)}>
                  {look.name || tr("profile.config", { index: i + 1 })}
                  {item.activeConfig === i && `, ${tr(loadout ? "profile.savedConfig" : "profile.activeConfig")}`}
                </Chip>
              ))}
            />
            <div className="flex flex-col gap-5 p-4">
              {loadout && mods && (
                <section className="flex flex-col gap-3" data-profile-mods>
                  <h3 className="m-0 text-sm font-semibold text-text-secondary">{tr("profile.equippedUpgrades")}</h3>
                  <div className="flex flex-wrap gap-2">
                    {mods.modConfigs.map((m, i) => (
                      <Chip key={i} pressed={modIndex === i} data-profile-mod-config={i} onClick={() => setModPick(i)}>
                        {m.name || tr("profile.modConfig", { index: i + 1 })}
                        {mods.activeModConfig === i && `, ${tr("profile.savedConfig")}`}
                      </Chip>
                    ))}
                  </div>
                  {!modConfig || modConfig.upgrades == null ? (
                    <p className="m-0 text-sm text-text-muted" data-profile-mods-unavailable>
                      {tr("profile.modsUnavailable")}
                    </p>
                  ) : modConfig.upgrades.length === 0 ? (
                    <p className="m-0 text-sm text-text-muted">{tr("profile.noEquippedUpgrades")}</p>
                  ) : (
                    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-3">
                      {modConfig.upgrades.map((u) => (
                        <div key={u.slot} className="flex items-center gap-3 rounded-[var(--radius-md)] border border-border bg-bg-raised p-2" data-profile-mod-slot={u.slot}>
                          <Thumb src={u.type ? db[u.type]?.imageUrl : null} />
                          <div className="flex min-w-0 flex-col">
                            <span className="text-xs text-text-muted">{tr("overlay.reward.slot", { index: u.slot + 1 })}</span>
                            <span className="break-words text-sm text-text-primary">{u.type ? name(u.type) : tr("profile.unresolvedMod")}</span>
                          </div>
                          <Badge className="ml-auto">{u.rank == null ? tr("profile.rankUnavailable") : tr("browse.rankValue", { value: u.rank })}</Badge>
                        </div>
                      ))}
                    </div>
                  )}
                </section>
              )}

              {!config ? (
                <p className="m-0 text-sm text-text-muted">{tr("profile.noAppearanceConfig")}</p>
              ) : (
                <div className="grid grid-cols-1 gap-6 xl:grid-cols-2">
                  <section>
                    <h3 className="mb-3 mt-0 text-sm font-semibold text-text-secondary">{tr("profile.cosmetics")}</h3>
                    {config.skins.length === 0 && <p className="m-0 text-sm text-text-secondary">{tr("profile.noCosmetics")}</p>}
                    <div className="grid gap-2">
                      {config.skins.map((skin) => {
                        const key = slotKey(skin.slot);
                        return (
                          <div key={skin.slot} className="flex items-center gap-3 rounded-[var(--radius-md)] border border-border bg-bg-raised p-2" data-profile-cosmetic-slot={skin.slot}>
                            <Thumb src={db[skin.type]?.imageUrl} />
                            <div className="flex min-w-0 flex-col">
                              <span className="text-xs text-text-muted">{key ? tr(key) : tr("overlay.reward.slot", { index: skin.slot })}</span>
                              <span className="break-words text-sm text-text-primary">{name(skin.type)}</span>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </section>
                  <div className="flex flex-col gap-5">
                    {colors.length === 0 && <p className="m-0 text-sm text-text-secondary">{tr("profile.noColors")}</p>}
                    {colors.map((group) => (
                      <section key={group}>
                        <h3 className="mb-3 mt-0 text-sm font-semibold text-text-secondary">{tr(GROUP_KEYS[group])}</h3>
                        <div className="grid grid-cols-2 gap-2">
                          {CHANNELS.map((channel) => {
                            const color = config.colors[group]?.[channel];
                            return (
                              color && (
                                <div key={channel} className="flex items-center gap-2 rounded-[var(--radius-md)] border border-border bg-bg-raised p-2" data-profile-color={`${group}-${channel}`}>
                                  {/* the swatch is the player's own colour, not a theme colour */}
                                  <span className="size-6 shrink-0 rounded-[var(--radius-sm)] border border-border-strong" style={{ backgroundColor: color }} aria-hidden="true" />
                                  <span className="flex min-w-0 flex-col">
                                    <span className="text-xs text-text-secondary">{group === "sigcol" ? tr("profile.colorChannel", { channel }) : tr(COLOR_KEYS[channel])}</span>
                                    <code className="text-xs text-text-primary">{color}</code>
                                  </span>
                                </div>
                              )
                            );
                          })}
                        </div>
                      </section>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </Card>
        </>
      )}
    </div>
  );
}
