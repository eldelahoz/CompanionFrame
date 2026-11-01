/** Thin progress bar. `value` is the filled share, `extra` an optional second segment after it (e.g. in progress). Both 0–100. */
export function Meter({ value, extra = 0, tone = "accent", label, className = "" }: { value: number; extra?: number; tone?: "accent" | "success" | "info" | "warning"; label?: string; className?: string }) {
  const v = Math.max(0, Math.min(100, value));
  const x = Math.max(0, Math.min(100 - v, extra));
  return (
    <span className={`ui-meter ui-meter--${tone} ${className}`} role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true}>
      <span style={{ width: `${v}%` }} />
      {x > 0 && <span className="ui-meter-extra" style={{ width: `${x}%` }} />}
    </span>
  );
}
