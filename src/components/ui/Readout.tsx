import type { ReactNode } from "react";
import { formatFixed } from "../../lib/format";
import { GameIcon } from "../icons";

export type Currency = "platinum" | "ducat" | "endo" | "credits" | "mastery";

const fmt = formatFixed;

/**
 * The signature value chip: tabular numerals with the currency glyph in front, so prices line up like an
 * instrument column in lists, tables and overlays. Use it for every platinum, ducat, endo and credit value.
 */
export function Readout({
  currency,
  value,
  digits = 0,
  label,
  placeholder,
  chip = false,
  size = "md",
  sign = false,
  locale,
  className = "",
  children,
}: {
  currency?: Currency;
  value: number | null | undefined;
  digits?: number;
  /** Shown when there is no value; without it a missing value renders nothing. */
  placeholder?: ReactNode;
  /** Short qualifier shown before the glyph, e.g. "EV". */
  label?: ReactNode;
  chip?: boolean;
  size?: "md" | "lg";
  /** Prefix + / − and tint gains (trades). */
  sign?: boolean;
  locale?: string;
  className?: string;
  /** Replaces the formatted number (e.g. an inline price input). */
  children?: ReactNode;
}) {
  if (value == null && children == null && placeholder == null) return null;
  const cls = ["ui-readout", chip && "ui-readout--chip", size === "lg" && "ui-readout--lg", sign && value != null && (value > 0 ? "ui-readout--up" : "ui-readout--down"), className].filter(Boolean).join(" ");
  const text = value == null ? placeholder : `${sign ? (value > 0 ? "+" : value < 0 ? "−" : "") : ""}${fmt(sign ? Math.abs(value) : value, digits, locale)}`;
  return (
    <span className={cls} data-readout={currency}>
      {label && <span className="ui-readout-label">{label}</span>}
      {currency && <GameIcon name={currency} label={currency} />}
      {children ?? <span>{text}</span>}
    </span>
  );
}
