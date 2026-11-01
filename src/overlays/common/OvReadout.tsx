import { GameIcon } from "../../components/icons";

/** The app's Readout for overlays: the currency's game art, then tabular numerals. */
export function OvReadout({ currency, value, size, dim }: { currency: "platinum" | "ducat"; value: number | string; size?: "lg"; dim?: boolean }) {
  return (
    <span className="ov-ro" data-size={size} data-dim={dim || undefined}>
      <GameIcon name={currency} className="ov-gi" />
      {typeof value === "number" ? value.toLocaleString() : value}
    </span>
  );
}
