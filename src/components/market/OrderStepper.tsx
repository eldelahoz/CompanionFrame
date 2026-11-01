import type { Translator } from "../../lib/i18n";

interface Props {
  value: number;
  min?: number;
  max?: number;
  /** Accessible name for the arrows, e.g. "price" or "quantity". */
  label: string;
  onChange: (next: number) => void;
  tr: Translator;
}

const ARROW =
  "flex h-3.5 w-4 cursor-pointer items-center justify-center rounded-[var(--radius-sm)] border-0 bg-transparent p-0 text-[9px] leading-none text-text-muted hover:bg-bg-hover hover:text-text-primary";

/** Inline number control: typed value + arrow steps. */
export default function OrderStepper({ value, min = 1, max = 99_999, label, onChange, tr }: Props) {
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const step = (d: number) => onChange(clamp((Number.isFinite(value) ? value : min) + d));
  return (
    <span className="inline-flex items-center gap-0.5">
      <input
        key={value /* remount so a cleared or garbage edit snaps back to the real value */}
        type="number"
        min={min}
        max={max}
        defaultValue={value}
        aria-label={tr("market.listedLabel", { label })}
        style={{ width: `${String(value).length + 3}ch` }}
        className="ui-input h-7 min-w-[3rem] px-1.5 text-right font-semibold tabular-nums [&::-webkit-inner-spin-button]:appearance-none"
        onBlur={(e) => {
          const raw = parseInt(e.currentTarget.value, 10);
          if (Number.isFinite(raw) && clamp(raw) !== value) onChange(clamp(raw));
          else e.currentTarget.value = String(value);
        }}
      />
      <span className="flex flex-col">
        <button type="button" className={ARROW} aria-label={tr("market.increaseLabel", { label })} onClick={() => step(1)}>
          &#9650;
        </button>
        <button type="button" className={ARROW} aria-label={tr("market.decreaseLabel", { label })} onClick={() => step(-1)}>
          &#9660;
        </button>
      </span>
    </span>
  );
}
