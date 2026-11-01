import { describe, expect, it } from "vitest";

import de from "./de.json";
import zh from "./zh.json";
import es from "./es.json";
import en from "./en.json";

// The package blocks deep imports through "exports", so glob the files directly.
const DICTS = import.meta.glob<Record<string, string>>(
  "../../node_modules/warframe-public-export-plus/dict.{en,de,zh,es}.json",
  { eager: true, import: "default" },
);
const officialDict = (locale: string): Record<string, string> =>
  DICTS[`../../node_modules/warframe-public-export-plus/dict.${locale}.json`];

const OFFICIAL_EN = officialDict("en");
const OFFICIAL_DE = officialDict("de");
const OFFICIAL_ZH = officialDict("zh");
const OFFICIAL_ES = officialDict("es");

type Term = {
  en: string;
  de: string;
  stem?: string;
  enPattern?: RegExp;
  ownChoice?: string;
  /** Absent where the export never renders the term alone in Chinese. */
  zh?: string;
  zhStem?: string;
  /** Absent where the export never renders the term alone in Spanish. */
  es?: string;
  esStem?: string;
  /** Why we deliberately differ from the Spanish export. */
  esOwnChoice?: string;
};

const englishPattern = (term: Term): RegExp =>
  term.enPattern ?? new RegExp(`\\b${term.en}(s|es)?\\b`, "i");

// Match Digital Extremes by default; ownChoice records deliberate differences.
const TERMS: Term[] = [
  { en: "Arbitration", de: "Arbitration", ownChoice: "German players say the English word", es: "Arbitraje", esOwnChoice: "the export says Arbitramiento, players say Arbitraje" },
  { en: "Relic", de: "Relikt", zh: "虚空遗物", zhStem: "遗物", es: "Reliquia" },
  { en: "Orokin Ducats", de: "Orokin Dukaten", zh: "奥罗金杜卡德金币", zhStem: "杜卡德", es: "Ducados orokin", esStem: "Ducados" },
  { en: "Rifle", de: "Gewehr", zh: "步枪", es: "Rifle" },
  { en: "Shotgun", de: "Schrot", zh: "霰弹枪", es: "Escopeta" },
  { en: "Melee", de: "Nahkampf", zh: "近战", es: "Cuerpo a cuerpo" },
  { en: "Vitus Essence", de: "Vitus-Essenz", zh: "生息精华", es: "Esencia Vitus" },
  { en: "The Circuit", de: "Der Rundkurs", stem: "Rundkurs", zh: "无尽回廊", es: "El Circuito", esStem: "Circuito" },
  { en: "Riven", de: "Riven", zh: "裂罅", es: "Riven", esOwnChoice: "the export says Agrietado, players say Riven" },
  { en: "Kuva", de: "Kuva", zh: "赤毒", es: "Kuva" },
  { en: "Endo", de: "Endo", zh: "内融核心", es: "Endo" },
  { en: "Nightwave", de: "Nightwave", zh: "午夜电波", es: "Onda Nocturna" },
  { en: "Railjack", de: "Railjack", zh: "航道星舰", es: "Railjack" },
  { en: "Cetus", de: "Cetus", zh: "希图斯", es: "Cetus" },
  { en: "Duviri", de: "Duviri", zh: "双衍王境", es: "Duviri" },
];

// Words DE only writes inside longer phrases, so they cannot be pinned against
// the export; the consistency check below is what keeps them honest.
const UNPINNED: Term[] = [
  { en: "Foundry", de: "Schmiede", es: "Fundición" },
  { en: "Platinum", de: "Platinum" },
  { en: "Ducats", de: "Dukaten", es: "Ducados" },
  { en: "Fissure", de: "Riss", es: "Fisura" },
  { en: "Bounty", de: "Auftrag", stem: "Auftr", enPattern: /\bBount(y|ies)\b/i, es: "Contrato", esStem: "Contrat" },
  { en: "Invasion", de: "Invasion", es: "Invasión", esStem: "Invasi" },
  { en: "Mastery", de: "Meisterschaft", es: "Maestría" },
  { en: "Arcane", de: "Arkana", stem: "Arkan", es: "Arcano", esStem: "Arcan" },
  { en: "Veiled", de: "Verschleiert", stem: "erschleiert", zh: "未揭示", es: "Velado" },
  { en: "Credits", de: "Credits", es: "Créditos" },
];

function shippedOfficial(term: Term, dict: Record<string, string>): Set<string> {
  const needle = term.en.toLowerCase();
  const shipped = new Set<string>();
  for (const [key, value] of Object.entries(OFFICIAL_EN)) {
    if (typeof value === "string" && value.trim().toLowerCase() === needle && dict[key]) {
      shipped.add(dict[key].trim());
    }
  }
  return shipped;
}

describe("german game terminology", () => {
  it("matches the game's own German unless a term says otherwise", () => {
    const wrong: string[] = [];

    for (const term of TERMS) {
      const shipped = shippedOfficial(term, OFFICIAL_DE);
      if (term.ownChoice) {
        if (shipped.has(term.de)) {
          wrong.push(`${term.en}: ownChoice "${term.ownChoice}" is stale, DE now agrees with us`);
        }
      } else if (shipped.size === 0) {
        wrong.push(`${term.en}: gone from the DE export, move it to UNPINNED or re-pin it`);
      } else if (!shipped.has(term.de)) {
        wrong.push(`${term.en}: we use "${term.de}", DE uses ${[...shipped].join(" / ")}`);
      }
    }

    expect(wrong).toEqual([]);
  });

  it("uses each term the same way everywhere", () => {
    // Catch half-migrated wording after a German term is chosen.
    const inconsistent: string[] = [];

    for (const term of [...TERMS, ...UNPINNED]) {
      const mentions = englishPattern(term);
      const stem = (term.stem ?? term.de).toLowerCase();
      for (const [key, value] of Object.entries(en)) {
        const german = de[key as keyof typeof de];
        if (german === undefined || !mentions.test(value)) continue;
        if (!german.toLowerCase().includes(stem)) {
          inconsistent.push(`${key}: English says ${term.en}, German drops "${term.de}"`);
        }
      }
    }

    expect(inconsistent).toEqual([]);
  });
});

describe("chinese game terminology", () => {
  const pinned = TERMS.filter((term) => term.zh !== undefined);

  it("matches the game's own Chinese", () => {
    const wrong: string[] = [];

    for (const term of pinned) {
      const shipped = shippedOfficial(term, OFFICIAL_ZH);
      if (shipped.size === 0) {
        wrong.push(`${term.en}: gone from the CN export, drop its zh pin`);
      } else if (!shipped.has(term.zh!)) {
        wrong.push(`${term.en}: we use "${term.zh}", DE uses ${[...shipped].join(" / ")}`);
      }
    }

    expect(wrong).toEqual([]);
  });

  it("uses each term the same way everywhere", () => {
    // Chinese is a partial catalogue, so an absent key is fine; a present key
    // that drops the agreed word is the half-migrated wording this catches.
    const inconsistent: string[] = [];

    for (const term of [...TERMS, ...UNPINNED]) {
      if (term.zh === undefined) continue;
      const mentions = englishPattern(term);
      const stem = term.zhStem ?? term.zh;
      for (const [key, value] of Object.entries(en)) {
        const chinese = zh[key as keyof typeof zh];
        if (chinese === undefined || !mentions.test(value)) continue;
        if (!chinese.includes(stem)) {
          inconsistent.push(`${key}: English says ${term.en}, Chinese drops "${term.zh}"`);
        }
      }
    }

    expect(inconsistent).toEqual([]);
  });
});

describe("spanish game terminology", () => {
  const pinned = TERMS.filter((term) => term.es !== undefined);

  it("matches the game's own Spanish unless a term says otherwise", () => {
    const wrong: string[] = [];

    for (const term of pinned) {
      const shipped = shippedOfficial(term, OFFICIAL_ES);
      if (term.esOwnChoice) {
        if (shipped.has(term.es!)) {
          wrong.push(`${term.en}: esOwnChoice "${term.esOwnChoice}" is stale, DE now agrees with us`);
        }
      } else if (shipped.size === 0) {
        wrong.push(`${term.en}: gone from the ES export, drop its es pin`);
      } else if (!shipped.has(term.es!)) {
        wrong.push(`${term.en}: we use "${term.es}", DE uses ${[...shipped].join(" / ")}`);
      }
    }

    expect(wrong).toEqual([]);
  });

  it("uses each term the same way everywhere", () => {
    const inconsistent: string[] = [];

    for (const term of [...TERMS, ...UNPINNED]) {
      if (term.es === undefined) continue;
      const mentions = englishPattern(term);
      const stem = (term.esStem ?? term.es).toLowerCase();
      for (const [key, value] of Object.entries(en)) {
        const spanish = es[key as keyof typeof es];
        if (spanish === undefined || !mentions.test(value)) continue;
        if (!spanish.toLowerCase().includes(stem)) {
          inconsistent.push(`${key}: English says ${term.en}, Spanish drops "${term.es}"`);
        }
      }
    }

    expect(inconsistent).toEqual([]);
  });
});
