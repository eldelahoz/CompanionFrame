import type { ButtonHTMLAttributes, ReactNode } from "react";

export type Tone = "neutral" | "accent" | "success" | "warning" | "danger" | "info";

/** Static status label. Colour carries meaning only; pair it with text or an icon. */
export function Badge({ tone = "neutral", icon, children, className = "", title }: { tone?: Tone; icon?: ReactNode; children?: ReactNode; className?: string; title?: string }) {
  return (
    <span className={`ui-badge ${tone !== "neutral" ? `ui-badge--${tone}` : ""} ${className}`} title={title}>
      {icon}
      {children}
    </span>
  );
}

/** Toggleable filter pill. */
export function Chip({ pressed, className = "", type = "button", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { pressed: boolean }) {
  return <button type={type} aria-pressed={pressed} className={`ui-chip ${className}`} {...rest} />;
}
