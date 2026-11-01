import type { ReactNode } from "react";

/** One setting: label and description on the left, the control on the right; stacks in a narrow card (ui.css). */
export function SettingRow({ label, description, control, htmlFor }: { label: ReactNode; description?: ReactNode; control: ReactNode; htmlFor?: string }) {
  return (
    <div className="ui-setting-row" data-setting-row>
      <div className="ui-setting-text">
        {htmlFor ? <label htmlFor={htmlFor}>{label}</label> : <span>{label}</span>}
        {description && <p>{description}</p>}
      </div>
      <div className="ui-setting-control">{control}</div>
    </div>
  );
}
