import type { ButtonHTMLAttributes, ReactNode } from "react";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "ghost" | "danger";
  size?: "md" | "sm";
  /** Shows a spinner and blocks clicks while an action runs. */
  loading?: boolean;
  icon?: ReactNode;
};

export function Button({ variant = "default", size = "md", loading = false, icon, className = "", children, type = "button", ...rest }: ButtonProps) {
  const cls = ["ui-btn", variant !== "default" && `ui-btn--${variant}`, size === "sm" && "ui-btn--sm", className].filter(Boolean).join(" ");
  return (
    <button type={type} className={cls} aria-busy={loading || undefined} {...rest}>
      {icon}
      {children}
    </button>
  );
}

type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label"> & {
  /** Required: names the button for screen readers and shows as its tooltip. */
  label: string;
  danger?: boolean;
  children: ReactNode;
};

export function IconButton({ label, danger = false, className = "", type = "button", children, ...rest }: IconButtonProps) {
  return (
    <button type={type} className={`ui-icon-btn ${danger ? "ui-icon-btn--danger" : ""} ${className}`} aria-label={label} data-tip={label} {...rest}>
      {children}
    </button>
  );
}
