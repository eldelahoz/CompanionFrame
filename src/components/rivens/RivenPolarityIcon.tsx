import { PolarityIcon, type Polarity } from "../icons";
import type { Translator } from "../../lib/i18n";

const KIND_BY_VALUE: Record<string, Polarity> = {
  AP_ATTACK: "madurai",
  AP_TACTIC: "naramon",
  AP_DEFENSE: "vazarin",
  AP_POWER: "zenurik",
  AP_WARD: "unairu",
  AP_PRECEPT: "penjaga",
  AP_UMBRA: "umbra",
};

/** The polarity glyph of a riven (`AP_*` game value or the lowercase name); nothing for an unknown one. */
export default function RivenPolarityIcon({ polarity, className = "", tr }: { polarity: string | null | undefined; className?: string; tr: Translator }) {
  const raw = (polarity ?? "").trim();
  const kind = KIND_BY_VALUE[raw] ?? (Object.values(KIND_BY_VALUE).includes(raw.toLowerCase() as Polarity) ? (raw.toLowerCase() as Polarity) : null);
  if (!kind) return null;
  const name = kind[0].toUpperCase() + kind.slice(1);
  return <PolarityIcon polarity={kind} label={tr("rivens.polarityLabel", { polarity: name })} className={`riven-polarity ${className}`} />;
}
