import { useMemo } from "react";
import ItemImage from "../ItemImage";
import { Badge, Card, EmptyState } from "../ui";
import type { Language, ItemDatabase } from "../../lib/ipc";
import type { Translator } from "../../lib/i18n";
import { resolveItem } from "../../lib/inventory/itemClassification";
import {
  PET_TRAIT_ORDER,
  petTraitLabelKey,
  resolvePetTrait,
  type PetImprint,
  type PetInstance,
  type PetTraitInfo,
  type PetTraits,
  parsePetGenetics,
} from "../../lib/inventory/petGenetics";
import type { ItemDbEntry, RawInventoryData } from "../../types/inventory";

interface Props {
  inventory: RawInventoryData | null;
  items: ItemDatabase;
  locale: Language;
  /** The view's search box. */
  query: string;
  tr: Translator;
}

function Trait({ info, tr }: { info: PetTraitInfo | null; tr: Translator }) {
  if (!info) return <span className="text-text-muted">{tr("common.none")}</span>;
  return (
    <span className="pet-trait">
      {/* the swatch is the pet's actual colour: game data, not decoration */}
      <span className="pet-swatch" style={info.hex ? { background: info.hex } : undefined} data-pet-swatch={info.hex ?? undefined} aria-hidden="true" />
      {info.label}
    </span>
  );
}

function PetGenetics({ dominant, recessive, locale, tr }: { dominant: PetTraits; recessive: PetTraits; locale: Language; tr: Translator }) {
  const rows = PET_TRAIT_ORDER.flatMap((kind) => {
    const d = resolvePetTrait(kind, dominant[kind], locale, tr);
    const r = resolvePetTrait(kind, recessive[kind], locale, tr);
    // a species without a tail or head has neither side
    return d || r ? [{ kind, d, r }] : [];
  });
  if (rows.length === 0) return <p className="text-xs text-text-muted">{tr("pet.traitsUnavailable")}</p>;
  return (
    <table className="pet-traits" aria-label={tr("pet.title")}>
      <thead>
        <tr>
          <th />
          <th>{tr("pet.dominant")}</th>
          <th>{tr("pet.recessive")}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(({ kind, d, r }) => (
          <tr key={kind}>
            <th scope="row">{tr(petTraitLabelKey(kind))}</th>
            <td>
              <Trait info={d} tr={tr} />
            </td>
            <td>
              <Trait info={r} tr={tr} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function PetMeta({ pet, locale, tr }: { pet: PetInstance; locale: Language; tr: Translator }) {
  return (
    <div className="pet-meta">
      {pet.isMale !== null && <Badge>{tr(pet.isMale ? "pet.male" : "pet.female")}</Badge>}
      <Badge tone="info">{pet.statusKey ? tr(pet.statusKey) : pet.statusLabel}</Badge>
      {pet.size !== null && <span>{tr("pet.size", { value: pet.size.toFixed(2) })}</span>}
      {pet.printsRemaining !== null && <span>{tr("pet.printsRemaining", { count: pet.printsRemaining })}</span>}
      {pet.hatchDate && <span>{tr("pet.hatched", { date: pet.hatchDate.toLocaleDateString(locale) })}</span>}
    </div>
  );
}

/** Kubrow and Kavat companions with their dominant/recessive genes, and stored imprints, searchable by name. */
export default function PetsInventory({ inventory, items, locale, query, tr }: Props) {
  const genetics = useMemo(() => parsePetGenetics(inventory), [inventory]);
  const db = items as unknown as Record<string, ItemDbEntry>;
  const q = query.trim().toLocaleLowerCase(locale);
  const species = (key: string) => {
    const item = resolveItem(key, db);
    return { name: item.displayName ?? item.name, image: item.imageUrl ?? null };
  };

  const pets = useMemo(
    () =>
      [...genetics.bySpecies]
        .flatMap(([key, list]) => list.map((pet, i) => ({ id: pet.instanceId ?? `${key}:${i}`, pet, ...species(key) })))
        .map((c) => ({ ...c, title: c.pet.name || c.name }))
        .filter((c) => `${c.title} ${c.name}`.toLocaleLowerCase(locale).includes(q))
        .sort((a, b) => a.title.localeCompare(b.title, locale)),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `species` only reads `db`
    [genetics, db, q, locale],
  );
  const prints = useMemo(
    () =>
      [...genetics.printsBySpecies]
        .map(([key, list]) => ({ key, name: species(key).name, list: list.filter((p: PetImprint) => `${species(key).name} ${p.name}`.toLocaleLowerCase(locale).includes(q)) }))
        .filter((g) => g.list.length > 0)
        .sort((a, b) => a.name.localeCompare(b.name, locale)),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `species` only reads `db`
    [genetics, db, q, locale],
  );

  return (
    <div className="pets-inventory" data-pets-inventory>
      <p className="pets-summary">{tr("pet.inventorySummary", { pets: genetics.totalPets, imprints: genetics.totalPrints })}</p>
      {pets.length === 0 && prints.length === 0 ? (
        <EmptyState title={tr(genetics.totalPets || genetics.totalPrints ? "pet.noMatches" : "pet.inventoryEmpty")} />
      ) : (
        <>
          <div className="pets-grid">
            {pets.map((c) => (
              <Card key={c.id} as="article" className="pet-card" data-pet-card={c.id}>
                <header>
                  <span className="pet-art">
                    <ItemImage src={c.image} alt={c.name} tr={tr} cls="h-full w-full" />
                  </span>
                  <div className="min-w-0">
                    <h3>{c.title}</h3>
                    <p>{c.name}</p>
                  </div>
                </header>
                <PetMeta pet={c.pet} locale={locale} tr={tr} />
                <PetGenetics dominant={c.pet.dominant} recessive={c.pet.recessive} locale={locale} tr={tr} />
              </Card>
            ))}
          </div>
          {prints.length > 0 && (
            <>
              <h2 className="pets-heading">{tr("pet.imprints")}</h2>
              <div className="pets-grid">
                {prints.flatMap((g) =>
                  g.list.map((p: PetImprint, i: number) => (
                    <Card key={`${g.key}:${p.instanceId ?? i}`} as="article" className="pet-card" data-pet-imprint={p.instanceId ?? `${g.key}:${i}`}>
                      <header>
                        <div className="min-w-0">
                          <h3>{tr("pet.imprintOf", { name: p.name })}</h3>
                          <p>{g.name}</p>
                        </div>
                      </header>
                      <div className="pet-meta">
                        {p.isMale !== null && <Badge>{tr(p.isMale ? "pet.male" : "pet.female")}</Badge>}
                        {p.size !== null && <span>{tr("pet.size", { value: p.size.toFixed(2) })}</span>}
                      </div>
                      <PetGenetics dominant={p.dominant} recessive={p.recessive} locale={locale} tr={tr} />
                    </Card>
                  )),
                )}
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
