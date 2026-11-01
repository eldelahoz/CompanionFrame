import { describe, expect, it } from "vitest";

import { buildCodexRows, sortCodexRows } from "./codexScans";
import { CODEX_EXTRA_INFO, CODEX_SCAN_REQUIREMENTS } from "../data/codexScanRequirements";

const BUTCHER = "/Lotus/Types/Enemies/Grineer/AIWeek/BladeSawman";
const TERRA_ELITE_CREWMAN = "/Lotus/Types/Enemies/Corpus/Venus/VenusHeavyEliteSpacemanAgent";
// The profile files this one under an /Avatars/ directory the wiki omits.
const TERRA_ELITE_CREWMAN_SCAN =
  "/Lotus/Types/Enemies/Corpus/Venus/Avatars/VenusHeavyEliteSpacemanAvatar";
const ROGUE_CONDROC =
  "/Lotus/Types/NeutralCreatures/Conservation/BirdOfPrey/UncommonBirdOfPreyAvatar";
// Extras-only, unmapped by CODEX_SCAN_AVATARS, and merged with a PNW twin.
const SCORPION_LEADER_SCAN =
  "/Lotus/Types/Enemies/Grineer/Narmer/Avatars/NarmerMacheteWomanLeaderAvatar";

describe("buildCodexRows", () => {
  it("ships a populated requirements table", () => {
    expect(Object.keys(CODEX_SCAN_REQUIREMENTS).length).toBeGreaterThan(500);
    expect(CODEX_SCAN_REQUIREMENTS[BUTCHER]).toMatchObject({
      name: "Butcher",
      scans: 20,
      faction: "grineer",
    });
  });

  it("matches profile avatar paths against wiki agent paths", () => {
    const rows = buildCodexRows([{ type: `${BUTCHER}Avatar`, count: 20 }]);
    const butcher = rows.find((row) => row.type === BUTCHER);
    expect(butcher).toMatchObject({ name: "Butcher", scanned: 20, required: 20, complete: true });
  });

  it("matches through the profile's extra /Avatars/ directory", () => {
    const rows = buildCodexRows([{ type: TERRA_ELITE_CREWMAN_SCAN, count: 3087 }]);
    expect(rows.find((row) => row.type === TERRA_ELITE_CREWMAN)).toMatchObject({
      name: "Terra Elite Crewman",
      scanned: 3087,
      complete: true,
    });
    // No leftover row under the raw internal name.
    expect(rows.some((row) => row.name.startsWith("Venus Heavy Elite"))).toBe(false);
  });

  it("keeps tileset variants apart instead of folding them into the base", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Enemies/Grineer/Desert/Avatars/RifleLancerAvatar", count: 9 },
    ]);
    expect(rows.find((row) => row.name === "Arid Lancer")?.scanned).toBe(9);
    expect(rows.find((row) => row.name === "Lancer")?.scanned).toBe(0);
  });

  it("a scan feeds only the wiki entry it matches at the strictest level", () => {
    // Both Nightwatch gunners share one loose stem; the /Avatars/ path is
    // Bombard's own key, so Gunner must stay untouched.
    const rows = buildCodexRows([
      {
        type: "/Lotus/Types/Enemies/Grineer/AIWeek/Avatars/NightwatchHeavyGunnerAvatar",
        count: 3,
      },
    ]);
    expect(rows.find((row) => row.name === "Nightwatch Bombard")).toMatchObject({
      scanned: 3,
      complete: true,
    });
    expect(rows.find((row) => row.name === "Nightwatch Gunner")).toMatchObject({
      scanned: 0,
      complete: false,
    });
  });

  it("suffix twins that are different enemies never share one scan count", () => {
    const rows = buildCodexRows([
      {
        type: "/Lotus/Types/Enemies/Grineer/InfestedMicroPlanet/GrineerShotgunSurvivorAvatar",
        count: 10,
      },
    ]);
    expect(rows.find((row) => row.name === "Trooper Survivor")).toMatchObject({
      scanned: 10,
      complete: true,
    });
    expect(rows.find((row) => row.name === "Lancer Survivor")).toMatchObject({ scanned: 0 });
  });

  it("same-name extras with different requirements stay separate rows", () => {
    const rows = buildCodexRows([]);
    const sacs = rows.filter((row) => row.name === "Mytocardia Sac");
    expect(sacs.map((row) => row.required).sort((a, b) => (a ?? 0) - (b ?? 0))).toEqual([5, 12]);
  });

  it("an extras entry's leader scans surface as their own Eximus row", () => {
    const rows = buildCodexRows([
      { type: ROGUE_CONDROC, count: 20 },
      { type: `${ROGUE_CONDROC}Leader`, count: 7 },
    ]);
    expect(rows.find((row) => row.name === "Rogue Condroc")?.scanned).toBe(20);
    expect(rows.find((row) => row.name === "Rogue Condroc Eximus")?.scanned).toBe(7);
  });

  it("splits leader avatars into their own Eximus entry", () => {
    const rows = buildCodexRows([
      { type: `${BUTCHER}Avatar`, count: 20 },
      { type: `${BUTCHER}AvatarLeader`, count: 3 },
    ]);
    expect(rows.find((row) => row.name === "Butcher")).toMatchObject({ scanned: 20 });
    // An Eximus does not inherit the base requirement: Butcher needs 20 scans,
    // its Eximus 3, which only ExportEnemies states.
    expect(rows.find((row) => row.name === "Butcher Eximus")).toMatchObject({
      scanned: 3,
      required: 3,
      complete: true,
    });
  });

  it("lists an Eximus the profile has never scanned at zero", () => {
    const rows = buildCodexRows([{ type: `${BUTCHER}Avatar`, count: 20 }]);
    expect(rows.find((row) => row.name === "Butcher Eximus")).toMatchObject({
      scanned: 0,
      required: 3,
      complete: false,
    });
  });

  it("lists never-scanned enemies at zero", () => {
    const rows = buildCodexRows([]);
    const butcher = rows.find((row) => row.type === BUTCHER);
    expect(butcher).toMatchObject({ scanned: 0, complete: false });
    expect(rows.length).toBeGreaterThan(500);
  });

  it("seeds codex extras so unscanned fragments and wildlife still list", () => {
    const rows = buildCodexRows([]);
    expect(Object.keys(CODEX_EXTRA_INFO).length).toBeGreaterThan(500);
    expect(rows.find((row) => row.name === "Rogue Condroc")).toMatchObject({
      scanned: 0,
      faction: "wildlife",
      complete: false,
    });
    expect(rows.filter((row) => row.faction === "lore").length).toBeGreaterThan(100);
  });

  it("completes conservation rows, which DE's export gives no requirement", () => {
    const rows = buildCodexRows([{ type: ROGUE_CONDROC, count: 20 }]);
    expect(rows.find((row) => row.name === "Rogue Condroc")).toMatchObject({
      scanned: 20,
      required: 20,
      complete: true,
    });
  });

  it("collapses a creature's male, female and base avatars into one row", () => {
    const rows = buildCodexRows([{ type: ROGUE_CONDROC, count: 20 }]);
    expect(rows.filter((row) => row.name === "Rogue Condroc")).toHaveLength(1);
  });

  it("appends scanned enemies the table does not know with a readable name", () => {
    const rows = buildCodexRows([{ type: "/Lotus/Types/Enemies/New/UnknownBossAvatar", count: 2 }]);
    const unknown = rows.find((row) => row.type === "/Lotus/Types/Enemies/New/UnknownBossAvatar");
    expect(unknown).toMatchObject({
      name: "Unknown Boss",
      scanned: 2,
      required: null,
      complete: null,
    });
  });

  it("keeps an unknown enemy's leader scans off its base row", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Enemies/New/UnknownBossAvatar", count: 2 },
      { type: "/Lotus/Types/Enemies/New/UnknownBossAvatarLeader", count: 7 },
    ]);
    expect(rows.find((row) => row.name === "Unknown Boss")?.scanned).toBe(2);
    expect(rows.find((row) => row.name === "Unknown Boss Eximus")?.scanned).toBe(7);
  });

  it("gives every row a unique key so keyed rendering cannot collide", () => {
    const rows = buildCodexRows([
      { type: `${BUTCHER}Avatar`, count: 20 },
      { type: `${BUTCHER}AvatarLeader`, count: 3 },
    ]);
    expect(new Set(rows.map((row) => row.type)).size).toBe(rows.length);
  });

  it("sorts rows by display name", () => {
    const rows = buildCodexRows([]);
    const names = rows.map((row) => row.name);
    expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b)));
  });
});

describe("sortCodexRows", () => {
  const rows = buildCodexRows([
    { type: `${BUTCHER}Avatar`, count: 15 },
    { type: "/Lotus/Types/Enemies/New/UnknownBossAvatar", count: 99 },
  ]);

  it("scans puts the highest raw counts first", () => {
    const sorted = sortCodexRows(rows, "scans");
    expect(sorted[0].scanned).toBe(99);
    expect(sorted[1].name).toBe("Butcher");
  });

  it("progress ranks partial completion above zero and unknown last", () => {
    const sorted = sortCodexRows(rows, "progress");
    const butcherIdx = sorted.findIndex((row) => row.name === "Butcher");
    const zeroIdx = sorted.findIndex((row) => row.scanned === 0);
    expect(butcherIdx).toBeLessThan(zeroIdx);

    const firstUnknown = sorted.findIndex((row) => row.required === null);
    expect(firstUnknown).toBeGreaterThan(-1);
    expect(sorted.slice(firstUnknown).every((row) => row.required === null)).toBe(true);
  });

  it("name sorts alphabetically whatever order it is handed", () => {
    const shuffled = sortCodexRows(rows, "scans");
    const sorted = sortCodexRows(shuffled, "name");
    expect(sorted.map((row) => row.name)).toEqual(
      [...shuffled.map((row) => row.name)].sort((a, b) => a.localeCompare(b)),
    );
  });
});

describe("codex entries whose scan lands on a world prop", () => {
  // The scan lands on the deco prop, so a row keyed only to the sculpture reads
  // 0 while the in-game codex is complete.
  it("credits an Ayatan sculpture scanned on its deco prop", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Objects/Gameplay/OroFusexADeco", count: 1 },
      { type: "/Lotus/Objects/Gameplay/OroFusexBDeco", count: 3 },
    ]);
    expect(rows.find((row) => row.name === "Ayatan Sah Sculpture")).toMatchObject({
      scanned: 1,
      required: 1,
      complete: true,
    });
    expect(rows.find((row) => row.name === "Ayatan Ayr Sculpture")).toMatchObject({
      scanned: 3,
      complete: true,
    });
  });

  it("credits an Ayatan star scanned on its deco prop", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Objects/Gameplay/OroFusexOrnamentADeco", count: 5 },
    ]);
    expect(rows.find((row) => row.name === "Ayatan Cyan Star")).toMatchObject({
      scanned: 5,
      required: 5,
      complete: true,
    });
    expect(rows.find((row) => row.name === "Ayatan Amber Star")?.scanned).toBe(0);
  });

  it("credits a lore fragment scanned on its deco prop", () => {
    const rows = buildCodexRows([
      {
        type: "/Lotus/Types/Lore/Fragments/SolarisFragments/EudicoLoreFragmentBDeco",
        count: 1,
      },
    ]);
    const fragment = rows.find(
      (row) => row.type === "/Lotus/Types/Lore/Fragments/SolarisFragments/EudicoLoreFragmentB",
    );
    expect(fragment).toMatchObject({ scanned: 1, complete: true });
  });

  it("leaves an unrelated gameplay prop alone", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Objects/Gameplay/CollectibleSeriesOneDeco", count: 4 },
    ]);
    const ayatan = rows.filter((row) => row.name.startsWith("Ayatan"));
    expect(ayatan.length).toBeGreaterThan(0);
    expect(ayatan.every((row) => row.scanned === 0)).toBe(true);
  });
});

describe("plants", () => {
  it("names a plant scan from the pickup item it disagrees with", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Items/Plants/NightCommonPlant", count: 670 },
      { type: "/Lotus/Types/Items/Plants/DayRarePlant", count: 52 },
    ]);
    expect(rows.find((row) => row.name === "Moonlight Threshcone")).toMatchObject({
      scanned: 670,
      faction: "objects",
    });
    expect(rows.find((row) => row.name === "Sunlight Jadeleaf")).toMatchObject({ scanned: 52 });
  });

  it("leaves plant completion unknown, since no source states a requirement", () => {
    const rows = buildCodexRows([]);
    const plant = rows.find((row) => row.name === "Lunar Pitcher");
    expect(plant).toMatchObject({ scanned: 0, required: null, complete: null });
  });

  it("lists every plant the in-game codex shows", () => {
    const names = new Set(buildCodexRows([]).map((row) => row.name));
    for (const plant of [
      "Sunlight Threshcone",
      "Moonlight Threshcone",
      "Sunlight Dragonlily",
      "Moonlight Dragonlily",
      "Sunlight Jadeleaf",
      "Moonlight Jadeleaf",
      "Vestan Moss",
      "Lunar Pitcher",
      "Frostleaf",
      "Dusklight Sarracenia",
      "Ruk's Claw",
    ]) {
      expect(names.has(plant)).toBe(true);
    }
  });
});

describe("wiki entries whose InternalName is not the scanned path", () => {
  const HEAVY_GUNNER_SCAN = "/Lotus/Types/Enemies/Orokin/OrokinHeavyFemaleAvatar";
  const WARDEN_SCAN = "/Lotus/Types/Enemies/Orokin/Gamemodes/CorruptedWardenAvatar";

  it("credits a scan whose path shares nothing with the wiki InternalName", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Enemies/Infested/AiWeek/Crawlers/CrawlerAvatar", count: 35 },
    ]);
    expect(rows.find((row) => row.name === "Crawler")).toMatchObject({ scanned: 35 });
  });

  // The wiki calls this one OrokinMinigunBombard; only DE's own agent-to-avatar
  // map pairs it with the OrokinHeavyFemaleAvatar path a profile reports.
  it("credits a scan on its own, with no rival entry to disambiguate against", () => {
    const rows = buildCodexRows([{ type: HEAVY_GUNNER_SCAN, count: 464 }]);
    expect(rows.find((row) => row.name === "Corrupted Heavy Gunner")?.scanned).toBe(464);
    expect(rows.find((row) => row.name === "Corrupted Warden")?.scanned).toBe(0);
  });

  it("keeps two entries that share one artwork file apart", () => {
    const rows = buildCodexRows([
      { type: WARDEN_SCAN, count: 3 },
      { type: HEAVY_GUNNER_SCAN, count: 464 },
    ]);
    expect(rows.find((row) => row.name === "Corrupted Warden")?.scanned).toBe(3);
    expect(rows.find((row) => row.name === "Corrupted Heavy Gunner")?.scanned).toBe(464);
  });
});

describe("entries that span two factions", () => {
  // ArchonBorealAvatar is Sentient and ArchonBorealAvatarPNW is Narmer, but the
  // codex lists one Archon Boreal.
  it("lists one row and files it under the faction most of its paths name", () => {
    const rows = buildCodexRows([]);
    const boreal = rows.filter((row) => row.name === "Archon Boreal");
    expect(boreal).toHaveLength(1);
    const targets = rows.filter((row) => row.name === "Corpus Target");
    expect(targets).toHaveLength(1);
    expect(targets[0].faction).toBe("corpus");
  });
});

describe("scan requirements", () => {
  // The export writes 5 when an avatar states no requirement, so the wiki count
  // is the one that stands wherever it says 5.
  it("keeps the wiki count where the export states its placeholder", () => {
    const rows = buildCodexRows([]);
    expect(rows.find((row) => row.name === "Rana Del")?.required).toBe(3);
    expect(rows.find((row) => row.name === "Terra Elite Embattor MOA")?.required).toBe(3);
  });

  it("reads the base and its Eximus as separate requirements", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Enemies/Grineer/Desert/Avatars/BladeSawmanAvatarLeader", count: 3 },
    ]);
    expect(rows.find((row) => row.name === "Arid Butcher")).toMatchObject({
      scanned: 0,
      required: 20,
    });
    expect(rows.find((row) => row.name === "Arid Butcher Eximus")).toMatchObject({
      scanned: 3,
      required: 3,
      complete: true,
    });
  });
});

describe("Eximus spellings", () => {
  // DE writes the Eximus spawn as both XAvatarLeader and XLeaderAvatar.
  it("reads both leader spellings as the same enemy's Eximus row", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Enemies/Orokin/RifleLancerAvatar", count: 9803 },
      { type: "/Lotus/Types/Enemies/Orokin/RifleLancerLeaderAvatar", count: 362 },
    ]);
    expect(rows.find((row) => row.name === "Corrupted Lancer")?.scanned).toBe(9803);
    expect(rows.find((row) => row.name === "Corrupted Lancer Eximus")?.scanned).toBe(362);
  });

  it("still reads the AvatarLeader spelling", () => {
    const rows = buildCodexRows([
      { type: "/Lotus/Types/Enemies/Orokin/OrokinBladeSawmanAvatar", count: 6379 },
      { type: "/Lotus/Types/Enemies/Orokin/OrokinBladeSawmanAvatarLeader", count: 153 },
    ]);
    expect(rows.find((row) => row.name === "Corrupted Butcher")?.scanned).toBe(6379);
    expect(rows.find((row) => row.name === "Corrupted Butcher Eximus")?.scanned).toBe(153);
  });

  it("does not duplicate the Eximus row of an extras-only LeaderAvatar scan", () => {
    const rows = buildCodexRows([{ type: SCORPION_LEADER_SCAN, count: 7 }]);
    const eximus = rows.filter((row) => row.scanned === 7 && row.name.endsWith("Eximus"));
    expect(eximus).toHaveLength(1);
    expect(eximus[0].name).toBe("Narmer Scorpion Eximus");
  });
});

describe("codex extras that merge into one display row", () => {
  // Of the merged Narmer paths only the PNW variant states an Eximus count.
  it("takes the Eximus requirement from whichever merged path states one", () => {
    const rows = buildCodexRows([{ type: SCORPION_LEADER_SCAN, count: 7 }]);
    expect(rows.find((row) => row.name === "Narmer Scorpion Eximus")).toMatchObject({
      scanned: 7,
      required: 3,
      complete: true,
    });
  });

  it("takes the artwork from whichever merged path carries one", () => {
    // No shipped group has an art-less first path, so inject that shape.
    const FIRST = "/Lotus/Types/Enemies/Test/ZzMergeArtAvatar";
    const SECOND = "/Lotus/Types/Enemies/Test/ZzMergeArtTwinAvatar";
    CODEX_EXTRA_INFO[FIRST] = { name: "Zz Merge Art", faction: "grineer", eximusScans: 3 };
    CODEX_EXTRA_INFO[SECOND] = { name: "Zz Merge Art", faction: "grineer", icon: "art.png" };
    try {
      const rows = buildCodexRows([]);
      expect(rows.find((row) => row.name === "Zz Merge Art")?.image).toBe("art.png");
      expect(rows.find((row) => row.name === "Zz Merge Art Eximus")?.image).toBe("art.png");
    } finally {
      delete CODEX_EXTRA_INFO[FIRST];
      delete CODEX_EXTRA_INFO[SECOND];
    }
  });
});

describe("export-verified Codex aliases", () => {
  it.each([
    "Narmer Crewman",
    "Taro Stropha Crewman",
    "Sensor Regulator",
    "Commander",
    "Narmer Lancer",
    "Gyre Eviscerator",
    "Ancient Disruptor",
  ])("shows the unscanned %s Eximus requirement", (name) => {
    expect(buildCodexRows([]).find((row) => row.name === `${name} Eximus`)).toMatchObject({
      scanned: 0,
      required: 3,
      complete: false,
    });
  });

  it.each([
    ["MarineLeaderAvatar", "Councilor Vay Hek"],
    ["NinjaLeaderAvatar", "Tyl Regor"],
  ])("treats STANDARD %s as its base boss", (avatar, name) => {
    const rows = buildCodexRows([
      {
        type: `/Lotus/Types/Enemies/Grineer/Vip/Avatars/${avatar}`,
        count: 3,
      },
    ]);
    expect(rows.find((row) => row.name === name)?.scanned).toBe(3);
    expect(rows.find((row) => row.name === `${name} Eximus`)).toBeUndefined();
  });
});
