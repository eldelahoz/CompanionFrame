import { Search } from "lucide-react";
import type { InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from "react";

/** Class for a bare text input when a component is overkill (legacy name kept for the ported views). */
export const INPUT = "ui-input";

export function Field({ label, children, className = "" }: { label: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={`ui-field ${className}`}>
      {label}
      {children}
    </label>
  );
}

export const Input = ({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) => <input className={`ui-input ${className}`} {...rest} />;

export function SearchInput({ className = "", label, ...rest }: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  return (
    <label className={`ui-search ${className}`}>
      <span className="sr-only">{label}</span>
      <Search aria-hidden="true" />
      <input type="search" className="ui-input" {...rest} />
    </label>
  );
}

type SelectOption<K extends string | number> = readonly [K, string];

export function Select<K extends string | number>({
  options,
  className = "",
  ...rest
}: Omit<SelectHTMLAttributes<HTMLSelectElement>, "children"> & { options: ReadonlyArray<SelectOption<K>> }) {
  return (
    <select className={`ui-select ${className}`} {...rest}>
      {options.map(([value, text]) => (
        <option key={value} value={value}>
          {text}
        </option>
      ))}
    </select>
  );
}

type ToggleProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & { label?: ReactNode };

/** On/off setting that applies immediately. */
export function Switch({ label, className = "", ...rest }: ToggleProps) {
  const input = <input type="checkbox" role="switch" className="ui-switch" {...rest} />;
  return label ? (
    <label className={`ui-check-label ${className}`}>
      {input}
      {label}
    </label>
  ) : (
    input
  );
}

/** Selection in a list or form (styled globally in components.css). */
export function Checkbox({ label, className = "", ...rest }: ToggleProps) {
  const input = <input type="checkbox" {...rest} />;
  return label ? (
    <label className={`ui-check-label ${className}`}>
      {input}
      {label}
    </label>
  ) : (
    input
  );
}
