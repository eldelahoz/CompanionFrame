import type { ReactNode } from "react";

/** Port of ThemeDropdown.svelte; the parent owns `open`. */
export default function ThemeDropdown({
  label,
  valueLabel,
  open,
  onOpenChange,
  disabled = false,
  children,
}: {
  label: string;
  valueLabel: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="relative">
      <button
        type="button"
        disabled={disabled}
        onClick={() => !disabled && onOpenChange(!open)}
        className="grid w-full grid-cols-[auto_1fr_auto] items-center gap-[0.6rem] rounded-[var(--radius-xl)] border border-[var(--ui-control-border)] bg-[var(--ui-control-bg)] px-3 py-[0.48rem] text-left text-text-secondary enabled:cursor-pointer disabled:cursor-default disabled:opacity-55"
      >
        <span>{label}</span>
        <strong className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-semibold text-text-primary">{valueLabel}</strong>
        <span className="inline-flex h-4 w-4 items-center justify-center text-text-muted">
          <svg className="h-[0.9rem] w-[0.9rem]" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
            <path d={open ? "M8 4l5 6H3z" : "M8 12L3 6h10z"} />
          </svg>
        </span>
      </button>
      {open && !disabled && (
        <div className="absolute left-0 right-0 top-[calc(100%+0.35rem)] z-[15] grid max-h-[17rem] overflow-y-auto rounded-[var(--radius-xl)] border border-border bg-[color-mix(in_srgb,var(--bg-base)_94%,transparent)] p-[0.45rem] [backdrop-filter:var(--ui-backdrop-blur)]">
          {children}
        </div>
      )}
    </div>
  );
}

const OPTION =
  "grid grid-cols-[auto_1fr] items-center gap-[0.65rem] border-0 border-l-2 px-[0.45rem] py-[0.48rem] text-left cursor-pointer hover:text-text-primary hover:border-l-accent hover:bg-[color-mix(in_srgb,var(--accent)_10%,transparent)]";

/** One row of a ThemeDropdown menu: four swatches and a label. */
export function ThemeOption({
  active,
  colors,
  label,
  onSelect,
}: {
  active: boolean;
  colors: { bgBase: string; bgRaised: string; textPrimary: string; accent: string };
  label: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={`${OPTION} ${active ? "border-l-accent bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] text-text-primary" : "border-l-transparent bg-transparent text-text-secondary"}`}
    >
      <span className="inline-flex gap-[0.22rem]">
        {[colors.bgBase, colors.bgRaised, colors.textPrimary, colors.accent].map((c, i) => (
          <span key={i} className="h-[0.86rem] w-[0.86rem] rounded-[var(--radius-sm)] border border-[var(--border-subtle)]" style={{ background: c }} />
        ))}
      </span>
      <span>{label}</span>
    </button>
  );
}
