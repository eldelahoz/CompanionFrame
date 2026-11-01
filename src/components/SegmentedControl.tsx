interface Props<T extends string> {
  value: T;
  options: ReadonlyArray<{ value: T; label: string }>;
  onChange: (value: T) => void;
}

export default function SegmentedControl<T extends string>({ value, options, onChange }: Props<T>) {
  return (
    <div className="inline-flex overflow-hidden rounded-md border border-[var(--ui-control-border)] bg-bg-surface text-xs">
      {options.map((o, i) => (
        <button
          key={o.value}
          type="button"
          data-segment-value={o.value}
          aria-pressed={value === o.value}
          onClick={() => onChange(o.value)}
          className={`px-2 py-1 transition-colors ${i > 0 ? "border-l border-border" : ""} ${
            value === o.value ? "bg-accent font-semibold text-bg-base" : "bg-bg-surface text-text-secondary hover:text-text-primary"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
