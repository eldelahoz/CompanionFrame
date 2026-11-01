/** Thin determinate bar. `value` is 0-100; `tone` colours the fill only where the state means something. */
export function Progress({ value, label, tone = "accent" }: { value: number; label: string; tone?: "accent" | "success" | "warning" }) {
  const pct = Math.min(100, Math.max(0, Number.isFinite(value) ? value : 0));
  return (
    <div className="ui-progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)} data-tone={tone}>
      <div style={{ width: `${pct}%` }} />
    </div>
  );
}
