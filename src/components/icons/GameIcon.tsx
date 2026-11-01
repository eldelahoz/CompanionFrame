
import { convertFileSrc, isTauri } from "@tauri-apps/api/core";
import { CircleDashed } from "lucide-react";
import { useEffect, useMemo, useState, type CSSProperties } from "react";

// Game art (© Digital Extremes) is never bundled: each icon is fetched at runtime from the first URL that loads
// (WARFRAME Wiki thumbnails first, then DE's public export on browse.wf / WFCD), else a generic glyph.
const wiki = (file: string, width?: number): string => `https://wiki.warframe.com/w/Special:FilePath/${encodeURIComponent(file)}${width ? `?width=${width}` : ""}`;
const de = (path: string): string => `https://browse.wf/Lotus/Interface/Icons/${path}`;
const WFCD = "https://cdn.warframestat.us/img/";

export type GameIconName =
  | "platinum"
  | "ducat"
  | "endo"
  | "credits"
  | "mastery"
  | "standing"
  | "rarity-common"
  | "rarity-uncommon"
  | "rarity-rare"
  | "rarity-legendary"
  | "damage-impact"
  | "damage-puncture"
  | "damage-slash"
  | "damage-heat"
  | "damage-cold"
  | "damage-electricity"
  | "damage-toxin"
  | "world-earth"
  | "world-vallis"
  | "world-cambion"
  | "world-zariman";

export type Polarity = "madurai" | "vazarin" | "naramon" | "zenurik" | "unairu" | "penjaga" | "umbra" | "koneksi" | "aura";
export type RelicTier = "lith" | "meso" | "neo" | "axi" | "requiem";
type Refinement = "intact" | "exceptional" | "flawless" | "radiant";

const W = 96;
const FILE: Record<GameIconName, string[]> = {
  platinum: [wiki("PlatinumLarge.png", W)],
  ducat: [wiki("OrokinDucats.png", W), de("StoreIcons/Currency/Ducat.png"), `${WFCD}Ducat.png`],
  endo: [wiki("Endo.png", W), de("StoreIcons/Currency/EndoIconRenderLarge.png")],
  credits: [wiki("Credits.png", W)],
  mastery: [wiki("MasteryAffinity64.png", W)],
  standing: [wiki("ReputationLarge.png", W)],
  "rarity-common": [wiki("IconCommon.png", W)],
  "rarity-uncommon": [wiki("IconUncommon.png", W)],
  "rarity-rare": [wiki("IconRare.png", W)],
  "rarity-legendary": [wiki("IconLegendary.png", W)],
  "damage-impact": [wiki("ImpactSymbol.png", W)],
  "damage-puncture": [wiki("PunctureSymbol.png", W)],
  "damage-slash": [wiki("SlashSymbol.png", W)],
  "damage-heat": [wiki("HeatSymbol.png", W)],
  "damage-cold": [wiki("ColdSymbol.png", W)],
  "damage-electricity": [wiki("ElectricitySymbol.png", W)],
  "damage-toxin": [wiki("ToxinSymbol.png", W)],
  "world-earth": [wiki("EarthCutout.png", W)],
  "world-vallis": [wiki("VenusCutout.png", W)],
  "world-cambion": [wiki("DeimosCutout.png", W)],
  "world-zariman": [wiki("ZarimanCutout.png", W)],
};

/** Single-colour silhouettes drawn with currentColor, so they follow the theme. */
const MASKED: ReadonlySet<GameIconName> = new Set(["standing"]);

interface IconProps {
  /** Accessible name; omit for a decorative icon next to its text. */
  label?: string;
  className?: string;
}

const a11y = (label?: string) => (label ? { role: "img", "aria-label": label } : { "aria-hidden": true as const });

/** First index worth trying per icon, so a dead primary URL is not retried by every instance. */
const failedFrom = new Map<string, number>();

function RemoteIcon({ urls: sources, masked = false, label, className = "" }: IconProps & { urls: readonly string[]; masked?: boolean }) {
  const key = sources.join("|");
  // In the app the backend fetches each URL once and serves it from the disk cache (the wiki redirect is not cached by the webview).
  const urls = useMemo(() => (isTauri() ? sources.map((u) => convertFileSrc(u, "gameimg")) : sources), [key]); // eslint-disable-line react-hooks/exhaustive-deps -- `key` stands for the list
  const [i, setI] = useState(() => failedFrom.get(key) ?? 0);
  const [loaded, setLoaded] = useState<string | null>(null);
  const next = () => {
    failedFrom.set(key, i + 1);
    setI(i + 1);
  };
  // A CSS mask cannot report a failed load, so probe the URL (CORS mode, like the mask fetch) before using it.
  useEffect(() => {
    if (!masked || i >= urls.length) return;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => setLoaded(urls[i]);
    img.onerror = () => {
      failedFrom.set(key, i + 1);
      setI(i + 1);
    };
    img.src = urls[i];
    return () => {
      img.onload = img.onerror = null;
    };
  }, [masked, i, key, urls]);

  if (i >= urls.length) return <CircleDashed className={`game-icon ${className}`} {...a11y(label)} />;
  if (!masked) return <img className={`game-icon ${className}`} src={urls[i]} alt={label ?? ""} draggable={false} onError={next} />;
  if (loaded !== urls[i]) return <span className={`game-icon ${className}`} {...a11y(label)} />;
  return <span className={`game-mask ${className}`} style={{ "--mask": `url("${loaded}")` } as CSSProperties} {...a11y(label)} />;
}

export function GameIcon({ name, label, className }: IconProps & { name: GameIconName }) {
  return <RemoteIcon key={name} urls={FILE[name]} masked={MASKED.has(name)} label={label} className={className} />;
}

export function PolarityIcon({ polarity, label, className }: IconProps & { polarity: Polarity }) {
  // The wiki has no aura glyph, so aura goes straight to the generic icon.
  const urls = polarity === "aura" ? [] : [wiki(`${polarity[0].toUpperCase()}${polarity.slice(1)}_Pol(xBlack).svg`)];
  return <RemoteIcon key={polarity} urls={urls} masked label={label} className={className} />;
}

const QUALITY_LETTER: Record<Refinement, string> = { exceptional: "A", flawless: "B", radiant: "C", intact: "D" };

export function RelicIcon({ tier, refinement = "intact", label, className }: IconProps & { tier: RelicTier; refinement?: Refinement }) {
  const dePath = `Relics/Relic${tier === "requiem" ? "Immortal" : tier[0].toUpperCase() + tier.slice(1)}${QUALITY_LETTER[refinement]}.png`;
  const wikiFile = `${tier[0].toUpperCase()}${tier.slice(1)}Relic${refinement[0].toUpperCase()}${refinement.slice(1)}.png`;
  return <RemoteIcon key={`${tier}-${refinement}`} urls={[wiki(wikiFile, W), de(dePath)]} label={label} className={className} />;
}

export const RARITY_ICON: Record<string, GameIconName> = {
  common: "rarity-common",
  uncommon: "rarity-uncommon",
  rare: "rarity-rare",
  legendary: "rarity-legendary",
};
