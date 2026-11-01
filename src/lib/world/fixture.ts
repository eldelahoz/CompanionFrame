// Realistic world state for plain-Vite dev and tests (no Tauri backend). Times are relative to `now`.
import type { Fissure, WorldState } from "../../bindings";

const iso = (ms: number) => new Date(ms).toISOString();
const MIN = 60_000;
const HOUR = 60 * MIN;

export function mockWorldState(now: number = Date.now()): WorldState {
  const fissure = (tier: string, missionType: string, node: string, mins: number, extra: Partial<Fissure> = {}): Fissure => ({
    expiry: iso(now + mins * MIN),
    tier,
    missionType,
    node,
    nodeId: node,
    isHard: false,
    isStorm: false,
    expired: false,
    ...extra,
  });
  const steelStart = now - 2 * 24 * HOUR;
  return {
    fissures: [
      fissure("Lith", "Capture", "Cinxia, Ceres", 38),
      fissure("Lith", "Survival", "Hydron, Sedna", 71),
      fissure("Meso", "Defense", "Helene, Saturn", 22),
      fissure("Neo", "Disruption", "Mot, Void", 95),
      fissure("Axi", "Exterminate", "Ose, Venus", 12),
      fissure("Requiem", "Mobile Defense", "Akkad, Eris", 55),
      fissure("Meso", "Spy", "Lares, Mercury", 47, { isHard: true }),
      fissure("Axi", "Survival", "Tuvul Commons, Zariman", 83, { isHard: true }),
      fissure("Neo", "Skirmish", "Gabii, Ceres", 64, { isStorm: true }),
    ],
    voidTrader: {
      activation: iso(now - 20 * HOUR),
      expiry: iso(now + 28 * HOUR),
      location: "Larunda Relay (Earth)",
      inventory: [
        { uniqueName: "/Lotus/Weapons/Tenno/Melee/PrimeFlail/AngstrumPrime", item: "Angstrum Prime", ducats: 350, credits: 150000, imageOverride: null },
        { uniqueName: "/Lotus/Types/Items/ShipDecos/Plushies/PlushyNecraLoid", item: "Necraloid Floof", ducats: 175, credits: 125000, imageOverride: null },
        { uniqueName: "/Lotus/Upgrades/Mods/Warframe/Prime/PrimeContinuity", item: "Primed Continuity", ducats: 330, credits: 175000, imageOverride: null },
        { uniqueName: "/Lotus/Types/Items/MiscItems/Forma", item: "Forma", ducats: 100, credits: 100000, imageOverride: null },
      ],
    },
    vaultTrader: {
      activation: iso(now - 6 * 24 * HOUR),
      expiry: iso(now + 9 * 24 * HOUR),
      location: "Maroo's Bazaar (Mars)",
      inventory: [
        { uniqueName: "/Lotus/Powersuits/Rhino/RhinoPrime", item: "RhinoPrime", ducats: null, credits: null, imageOverride: null },
        { uniqueName: "/Lotus/Weapons/Tenno/Rifle/BoltoPrime", item: "BoltoPrime", ducats: null, credits: null, imageOverride: null },
      ],
    },
    sortie: {
      id: "mock-sortie",
      activation: iso(now - 6 * HOUR),
      expiry: iso(now + 10 * HOUR),
      boss: "Alad V",
      missions: [
        { node: "Cinxia, Ceres", mission: "Exterminate", modifier: "Enemy Elemental Enhancement: Heat" },
        { node: "Hydron, Sedna", mission: "Defense", modifier: "Eximus Stronghold" },
        { node: "Mot, Void", mission: "Disruption", modifier: "Augmented Enemy Armor" },
      ],
    },
    archonHunt: {
      id: "mock-archon",
      activation: iso(now - 2 * 24 * HOUR),
      expiry: iso(now + 5 * 24 * HOUR),
      boss: "Archon Nira",
      missions: [
        { node: "Cinxia, Ceres", mission: "Assassination" },
        { node: "Hydron, Sedna", mission: "Defense" },
        { node: "Mot, Void", mission: "Exterminate" },
      ],
    },
    nightwave: {
      activation: iso(now - 20 * 24 * HOUR),
      expiry: iso(now + 40 * 24 * HOUR),
      season: 16,
      phase: 1,
      affiliationTag: "RadioLegionIntermission16Syndicate",
      challenges: [
        { id: "nw-d1", name: "SeasonDailyKills", title: "Eliminator", description: "Kill 50 enemies", standing: 1000, requiredCount: 50, isDaily: true, isElite: false, activation: iso(now - 4 * HOUR), expiry: iso(now + 20 * HOUR) },
        { id: "nw-w1", name: "SeasonWeeklySpy", title: "Silent Partner", description: "Complete 3 Spy missions", standing: 4500, requiredCount: 3, isDaily: false, isElite: false, activation: iso(now - 2 * 24 * HOUR), expiry: iso(now + 4 * 24 * HOUR) },
        { id: "nw-e1", name: "SeasonWeeklyHard", title: "Elite Eliminator", description: "Kill 500 enemies on Steel Path", standing: 7000, requiredCount: 500, isDaily: false, isElite: true, activation: iso(now - 2 * 24 * HOUR), expiry: iso(now + 4 * 24 * HOUR) },
      ],
    },
    descents: null,
    calendarSeason: null,
    alerts: [
      { id: "mock-alert", activation: iso(now - HOUR), expiry: iso(now + 5 * HOUR), node: "Lares, Mercury", mission: "Defense", faction: "Grineer", minLevel: 12, maxLevel: 18, credits: 12000, items: [{ name: "Orokin Catalyst Blueprint", count: 1 }] },
    ],
    steelPath: {
      currentReward: { name: "Kitgun Riven Mod", cost: 75 },
      activation: iso(steelStart),
      expiry: iso(steelStart + 7 * 24 * HOUR),
      rotation: [{ name: "Kitgun Riven Mod", cost: 75 }],
      upcoming: [
        { name: "3x Forma", cost: 75, activation: iso(now + 5 * 24 * HOUR), expiry: iso(now + 12 * 24 * HOUR), weekOffset: 1 },
        { name: "Zaw Riven Mod", cost: 75, activation: iso(now + 12 * 24 * HOUR), expiry: iso(now + 19 * 24 * HOUR), weekOffset: 2 },
      ],
      evergreens: [{ name: "Veiled Riven Cipher", cost: 20 }],
    },
    duviriCycle: {
      state: "Joy",
      nextState: "Anger",
      expiry: iso(now + 83 * MIN),
      choices: [
        { category: "normal", choices: ["Braton", "Lato", "Skana", "Paris"] },
        { category: "hard", choices: ["Soma Prime", "Sicarus", "Gram", "Dread"] },
      ],
    },
    earthCycle: { isDay: true, timeLeft: "", expiry: iso(now + 2 * HOUR + 14 * MIN) },
    cetusCycle: { isDay: true, timeLeft: "", expiry: iso(now + 2 * HOUR + 14 * MIN) },
    vallisCycle: { isWarm: false, timeLeft: "", expiry: iso(now + 6 * MIN + 40_000) },
    cambionCycle: { active: "fass", timeLeft: "", expiry: iso(now + 2 * HOUR + 14 * MIN) },
    invasions: [
      {
        id: "inv1",
        node: "Cassini, Saturn",
        desc: null,
        attacker: { faction: "Grineer", reward: { items: [], countedItems: [{ count: 1, type: "Fieldron" }], credits: 0 } },
        defender: { faction: "Corpus", reward: { items: [], countedItems: [{ count: 1, type: "Detonite Injector" }], credits: 0 } },
        vsInfestation: false,
        completion: 62.4,
        completed: false,
      },
      {
        id: "inv2",
        node: "Kiliken, Venus",
        desc: null,
        attacker: { faction: "Infested", reward: { items: [], countedItems: [], credits: 0 } },
        defender: { faction: "Corpus", reward: { items: [], countedItems: [{ count: 3, type: "Mutagen Mass" }], credits: 30000 } },
        vsInfestation: true,
        completion: 18.9,
        completed: false,
      },
    ],
    bounties: [
      {
        syndicate: "Ostrons",
        syndicateKey: "CetusSyndicate",
        expiry: iso(now + 2 * HOUR + 14 * MIN),
        jobs: [
          { type: "Attrition Bounty", enemyLevels: [5, 15], tierIndex: null, standingStages: [1000, 1500, 2000, 2500, 3000], minMr: 0, challengeDesc: null },
          { type: "Assassinate Bounty", enemyLevels: [20, 40], tierIndex: null, standingStages: [1500, 2000], minMr: 2, challengeDesc: null },
        ],
      },
      {
        syndicate: "The Holdfasts",
        syndicateKey: "ZarimanSyndicate",
        expiry: iso(now + 5 * HOUR),
        jobs: [
          { type: "Exterminate", enemyLevels: [50, 55], tierIndex: 0, standingStages: [1000], minMr: 0, challengeDesc: "Kill 30 Enemies with Melee" },
          { type: "Survival", enemyLevels: [60, 65], tierIndex: 1, standingStages: [2000], minMr: 0, challengeDesc: null },
        ],
      },
    ],
    bountyRotation: "B",
    dailyDeals: [
      {
        uniqueName: "/Lotus/Weapons/Tenno/Melee/Glaives/Boomerang/BoomerangWeapon",
        item: "Kestrel",
        imageOverride: null,
        discount: 50,
        originalPrice: 150,
        salePrice: 75,
        total: 300,
        sold: 97,
        expiry: iso(now + 14 * HOUR),
      },
    ],
  };
}
