import { useRef, type KeyboardEvent, type ReactNode } from "react";

interface TabItem<K extends string> {
  key: K;
  label: ReactNode;
  count?: number;
  icon?: ReactNode;
}

/** Underline tabs with arrow-key navigation (roving focus). */
export function Tabs<K extends string>({ items, active, onSelect, label, className = "" }: { items: ReadonlyArray<TabItem<K>>; active: K; onSelect: (key: K) => void; label: string; className?: string }) {
  const list = useRef<HTMLDivElement>(null);
  const onKeyDown = (e: KeyboardEvent) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const i = items.findIndex((t) => t.key === active);
    const next = items[(i + step + items.length) % items.length];
    onSelect(next.key);
    list.current?.querySelector<HTMLElement>(`[data-tab="${next.key}"]`)?.focus();
  };
  return (
    <div ref={list} role="tablist" aria-label={label} className={`ui-tabs ${className}`} onKeyDown={onKeyDown}>
      {items.map((t) => (
        <button key={t.key} type="button" role="tab" data-tab={t.key} aria-selected={t.key === active} tabIndex={t.key === active ? 0 : -1} className="ui-tab" onClick={() => onSelect(t.key)}>
          {t.icon}
          {t.label}
          {t.count !== undefined && <span className="ui-tab-count">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}

/** Small exclusive choice (2–4 options) that applies immediately. */
export function Segmented<K extends string>({ items, active, onSelect, label }: { items: ReadonlyArray<TabItem<K>>; active: K; onSelect: (key: K) => void; label: string }) {
  return (
    <div className="ui-seg" role="group" aria-label={label}>
      {items.map((t) => (
        <button key={t.key} type="button" aria-pressed={t.key === active} onClick={() => onSelect(t.key)}>
          {t.icon}
          {t.label}
        </button>
      ))}
    </div>
  );
}
