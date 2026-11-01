// Realistic relic data for plain-Vite dev and screenshots; the real database comes from the backend.
import type { RelicDatabase, RelicGroup, RelicReward } from "./types";
import { RELIC_QUALITY_MODES } from "../../../config/shared/relicPlannerView";

// Intact/Exceptional/Flawless/Radiant chances per slot: 3 commons, 2 uncommons, 1 rare.
const CHANCES = {
  intact: [25.33, 25.33, 25.33, 11, 11, 2],
  exceptional: [23.33, 23.33, 23.33, 13, 13, 4],
  flawless: [20, 20, 20, 17, 17, 6],
  radiant: [16.67, 16.67, 16.67, 20, 20, 10],
} as const;
const RARITY = ["Common", "Common", "Common", "Uncommon", "Uncommon", "Rare"];
const DUCATS: Record<string, number> = { Common: 15, Uncommon: 45, Rare: 100 };
const COLOURS = ["#c9a54a", "#4a9ec9", "#8a6bc9", "#5aa86b", "#c96b6b", "#aaa"];

const svg = (fill: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><circle cx="32" cy="32" r="28" fill="${fill}"/></svg>`)}`;

// [tier, code, vaulted, rewards in drop-table order (3 common, 2 uncommon, rare)]
const SPEC: Array<[string, string, boolean, string[]]> = [
  ["Lith", "A1", true, ["Forma Blueprint", "Akstiletto Prime Barrel", "Ash Prime Systems", "Bo Prime Handle", "Paris Prime String", "Nikana Prime Blade"]],
  ["Lith", "B2", false, ["Forma Blueprint", "Burston Prime Barrel", "Carrier Prime Cerebrum", "Rhino Prime Neuroptics", "Loki Prime Systems", "Rhino Prime Chassis"]],
  ["Lith", "C3", false, ["Braton Prime Receiver", "Forma Blueprint", "Dual Kamas Prime Blade", "Frost Prime Neuroptics", "Mag Prime Systems", "Saryn Prime Chassis"]],
  ["Lith", "D4", true, ["Forma Blueprint", "Fang Prime Handle", "Kavasa Prime Buckle", "Orthos Prime Blade", "Reaper Prime Blade", "Valkyr Prime Neuroptics"]],
  ["Lith", "G1", false, ["Forma Blueprint", "Nova Prime Systems", "Spira Prime Pouch", "Sicarus Prime Barrel", "Soma Prime Barrel", "Trinity Prime Chassis"]],
  ["Meso", "B1", false, ["Forma Blueprint", "Glaive Prime Blade", "Hydroid Prime Chassis", "Nami Skyla Prime Handle", "Octavia Prime Blueprint", "Volt Prime Systems"]],
  ["Meso", "C2", true, ["Forma Blueprint", "Ember Prime Neuroptics", "Kronen Prime Handle", "Nekros Prime Systems", "Scindo Prime Blade", "Rhino Prime Systems"]],
  ["Meso", "F3", false, ["Boar Prime Receiver", "Forma Blueprint", "Equinox Prime Chassis", "Gara Prime Systems", "Mesa Prime Blueprint", "Wukong Prime Neuroptics"]],
  ["Meso", "N5", false, ["Forma Blueprint", "Ballistica Prime String", "Dakra Prime Handle", "Latron Prime Stock", "Oberon Prime Systems", "Vauban Prime Chassis"]],
  ["Meso", "S4", false, ["Forma Blueprint", "Banshee Prime Systems", "Despair Prime Blade", "Hikou Prime Pouch", "Ivara Prime Neuroptics", "Titania Prime Neuroptics"]],
  ["Neo", "A2", true, ["Forma Blueprint", "Akbolto Prime Barrel", "Bronco Prime Receiver", "Cobra & Crane Prime Blade", "Rhino Prime Chassis", "Limbo Prime Systems"]],
  ["Neo", "N6", false, ["Forma Blueprint", "Atlas Prime Neuroptics", "Dual Zoren Prime Blade", "Inaros Prime Chassis", "Khora Prime Systems", "Wisp Prime Neuroptics"]],
  ["Neo", "R1", false, ["Forma Blueprint", "Baruuk Prime Chassis", "Gauss Prime Neuroptics", "Hildryn Prime Systems", "Revenant Prime Chassis", "Zakti Prime Barrel"]],
  ["Neo", "V8", false, ["Forma Blueprint", "Burston Prime Stock", "Chroma Prime Systems", "Equinox Prime Neuroptics", "Mirage Prime Chassis", "Nezha Prime Blueprint"]],
  ["Axi", "A7", false, ["Forma Blueprint", "Ankyros Prime Gauntlet", "Garuda Prime Chassis", "Gyre Prime Systems", "Harrow Prime Neuroptics", "Kogake Prime Boot"]],
  ["Axi", "C12", true, ["Forma Blueprint", "Lavos Prime Neuroptics", "Nidus Prime Blueprint", "Oberon Prime Neuroptics", "Protea Prime Chassis", "Yareli Prime Systems"]],
  ["Axi", "L4", false, ["Forma Blueprint", "Lex Prime Barrel", "Lokes Prime Systems", "Nyx Prime Neuroptics", "Titania Prime Systems", "Voruna Prime Chassis"]],
  ["Axi", "S3", false, ["Forma Blueprint", "Nikana Prime Hilt", "Rhino Prime Neuroptics", "Scindo Prime Handle", "Scindo Prime Blade", "Rhino Prime Systems"]],
  ["Requiem", "I", false, ["Fass Rune", "Jahu Rune", "Khra Rune", "Lohk Rune", "Netra Rune", "Xata Rune"]],
  ["Requiem", "II", false, ["Ris Rune", "Vome Rune", "Fass Rune", "Jahu Rune", "Lohk Rune", "Oull Rune"]],
];

const PROJECTION = "/Lotus/Types/Game/Projections/";
const SUFFIX = { intact: "Bronze", exceptional: "Silver", flawless: "Gold", radiant: "Platinum" } as const;

function reward(name: string, slot: number, quality: keyof typeof CHANCES): RelicReward {
  const rarity = RARITY[slot];
  return {
    name,
    uniqueName: `/Lotus/Types/Recipes/${name.replace(/[^A-Za-z]/g, "")}`,
    imageUrl: svg(COLOURS[slot % COLOURS.length]),
    rarity,
    chance: CHANCES[quality][slot],
    urlName: name.toLowerCase().replace(/[^a-z]+/g, "_"),
    wfmId: null,
    ducats: name === "Forma Blueprint" ? null : DUCATS[rarity],
  };
}

function group([tier, code, vaulted, names]: (typeof SPEC)[number]): RelicGroup {
  const key = `${tier} ${code}`;
  return {
    key,
    name: key,
    tier,
    code,
    vaulted,
    imageUrl: null,
    qualities: Object.fromEntries(
      RELIC_QUALITY_MODES.map((q) => [
        q,
        {
          uniqueName: `${PROJECTION}${tier}${code}${SUFFIX[q]}`,
          rewards: names.map((n, slot) => reward(n, slot, q)),
        },
      ]),
    ),
  };
}

export const MOCK_RELIC_DB: RelicDatabase = {
  groups: Object.fromEntries(SPEC.map((s) => [`${s[0]} ${s[1]}`, group(s)])),
  byUniqueName: Object.fromEntries(
    SPEC.flatMap(([tier, code]) =>
      RELIC_QUALITY_MODES.map((q) => [
        `${PROJECTION}${tier}${code}${SUFFIX[q]}`,
        { groupKey: `${tier} ${code}`, quality: q },
      ]),
    ),
  ),
};

/** A small owned inventory in the raw `MiscItems` shape the inventory port will deliver. */
export const MOCK_RELIC_INVENTORY: Record<string, unknown> = {
  MiscItems: [
    ["Lith", "A1", "intact", 3],
    ["Lith", "A1", "radiant", 1],
    ["Lith", "B2", "intact", 8],
    ["Meso", "C2", "flawless", 2],
    ["Meso", "F3", "intact", 5],
    ["Neo", "A2", "exceptional", 1],
    ["Neo", "N6", "intact", 12],
    ["Axi", "A7", "radiant", 2],
    ["Axi", "S3", "intact", 4],
    ["Axi", "S3", "radiant", 1],
  ].map(([tier, code, q, n]) => ({
    ItemType: `${PROJECTION}${tier}${code}${SUFFIX[q as keyof typeof SUFFIX]}`,
    ItemCount: n,
  })),
};
