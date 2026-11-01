// Realistic mastery fixture for the Vite dev server and tests (outside Tauri nothing computes it).
import type { MasteryItem, MasteryProgress } from "../bindings";

const svg = (fill: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect x="12" y="12" width="72" height="72" rx="14" fill="${fill}"/></svg>`)}`;

// Blueprint, Systems and Chassis of a set, `held` per part (the roadmap prices what is missing).
const parts = (item: string, held: boolean[]): MasteryItem["components"] =>
  ["Blueprint", "Systems", "Chassis"].map((part, i) => ({ name: part, uniqueName: `/Lotus/Mock/${item.replace(/\s+/g, "")}/${part}`, tradable: true, itemCount: 1, ownedCount: held[i] ? 1 : 0, owned: held[i], drops: [] }));

type Seed = [name: string, category: string, rank: number, maxRank: number, extra?: Partial<MasteryItem>];

const SEEDS: Seed[] = [
  ["Excalibur", "Warframes", 30, 30],
  ["Ash Prime", "Warframes", 30, 30, { isPrime: true, vaulted: true }],
  ["Mag", "Warframes", 18, 30],
  ["Rhino Prime", "Warframes", 0, 30, { isPrime: true, vaulted: true, masteryXpRemaining: 6000, components: parts("Rhino Prime", [true, true, false]) }],
  ["Khora", "Warframes", 0, 30, { masteryXpRemaining: 6000 }],
  ["Boltor Prime", "Primary", 30, 30, { isPrime: true }],
  ["Soma Prime", "Primary", 12, 30, { isPrime: true }],
  ["Tenet Arca Plasmor", "Primary", 0, 40, { masteryXpRemaining: 4000 }],
  ["Lato", "Secondary", 30, 30],
  ["Akbronco Prime", "Secondary", 7, 30, { isPrime: true }],
  ["Gram Prime", "Melee", 30, 30, { isPrime: true }],
  ["Sarofang Prime", "Melee", 21, 30, { isPrime: true }],
  ["Kogake Prime", "Melee", 0, 30, { masteryXpRemaining: 3000 }],
  ["Carrier Prime", "Companions", 30, 30, { isPrime: true }],
  ["Venari", "Companions", 0, 30, { masteryXpRemaining: 6000 }],
  ["Odonata", "Archwing", 30, 30],
  ["Amesha", "Archwing", 0, 30, { masteryXpRemaining: 6000 }],
  ["Mote Prism", "Amps", 30, 30],
  ["Bonewidow", "Necramech", 10, 40],
  ["Bad Baby", "Misc", 0, 30, { masteryXpRemaining: 6000 }],
];

/** Public art by file name (@wfcd/items on cdn.warframestat.us), as in production; unlisted items keep a flat tile. */
const ART: Record<string, string> = {"Amesha": "Amesha.png", "Odonata": "Odonata.png", "Kogake Prime": "KogakePrime.png", "Sarofang Prime": "SarofangPrime.png", "Ash Prime": "AshPrime.png", "Soma Prime": "SomaPrime.png", "Tenet Arca Plasmor": "CrpBEArcaPlasmor.png", "Akbronco Prime": "AkbroncoPrime.png", "Lato": "Lato.png", "Carrier Prime": "CarrierPrime.png", "Bonewidow": "ThanoMech.png", "Excalibur": "Excalibur.png", "Rhino Prime": "RhinoPrime.png", "Boltor Prime": "BoltorPrime.png", "Khora": "Khora.png", "Mag": "Mag.png", "Bad Baby": "HoverboardSolarisADeck.png", "Gram Prime": "GramPrime.png", "Mote Prism": "SentTrainingAmpBarrel.png"};
let tint = 0;
const items: MasteryItem[] = SEEDS.map(([name, category, rank, maxRank, extra = {}]) => {
  const owned = rank > 0;
  const perRank = ["Warframes", "Companions", "Necramech"].includes(category) || name === "Amesha" ? 200 : 100;
  const status = owned ? (rank >= maxRank ? "mastered" : "progress") : "missing";
  const masteryXp = rank * perRank;
  return {
    name,
    uniqueName: `/Lotus/Mock/${name.replace(/\s+/g, "")}`,
    category,
    imageUrl: ART[name] ? `https://cdn.warframestat.us/img/${ART[name]}` : svg(`hsl(${(tint += 37) % 360} 45% 48%)`),
    isPrime: false,
    masteryReq: name.includes("Prime") ? 0 : 5,
    vaulted: false,
    tradable: false,
    keywords: [],
    debugReason: "",
    status,
    rank,
    maxRank,
    currentlyOwned: owned,
    masteryXp,
    masteryXpRemaining: Math.max(0, maxRank * perRank - masteryXp),
    components: owned
      ? []
      : ["Blueprint", "Chassis", "Systems"].map((part, i) => ({
          name: part,
          uniqueName: `/Lotus/Mock/${name}/${part}`,
          tradable: true,
          itemCount: 1,
          ownedCount: i === 0 ? 1 : 0,
          owned: i === 0,
          drops: [],
        })),
    ...extra,
  };
});

const count = (s: MasteryItem["status"], category?: string) =>
  items.filter((i) => i.status === s && (!category || i.category === category)).length;

export const MOCK_MASTERY: MasteryProgress = {
  items,
  stats: {
    total: items.length,
    mastered: count("mastered"),
    inProgress: count("progress"),
    missing: count("missing"),
    byCategory: Object.fromEntries(
      [...new Set(items.map((i) => i.category))].map((c) => [
        c,
        {
          total: items.filter((i) => i.category === c).length,
          mastered: count("mastered", c),
          inProgress: count("progress", c),
          missing: count("missing", c),
        },
      ]),
    ),
    profileMastery: { rank: 24, percentToNext: 38.5, totalXp: 1_540_000, xpIntoRank: 110_000, xpForNext: 295_000, testReady: false },
    completion: {
      starChart: {
        normal: { done: 241, total: 252 },
        junctions: { done: 14, total: 17 },
        steelPath: { done: 120, total: 252 },
        steelPathJunctions: { done: 5, total: 17 },
      },
      intrinsics: { railjack: { done: 38, total: 50 }, drifter: { done: 24, total: 40 } },
    },
  },
  fromProfile: false,
};
