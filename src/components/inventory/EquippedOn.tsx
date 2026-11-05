import { Shirt } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import type { EquippedOn } from "../../types/inventory";

interface Props {
  list: readonly EquippedOn[];
  /** "pill" on the card (icon and count), "link" in the list ("On N items"). */
  variant: "pill" | "link";
  tr: Translator;
}

/** Button that opens a small popover with the gear a mod or arcane is fitted on. Escape and an outside click close it. */
export default function EquippedPopover({ list, variant, tr }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const popId = useId();
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      setOpen(false);
      button.current?.focus();
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  const label = tr("inventory.equipped.label", { count: list.length });
  return (
    <span className={`inv-equip inv-equip--${variant}`} ref={root} data-inventory-equipped>
      <button ref={button} type="button" className="inv-equip-btn" aria-label={label} aria-expanded={open} aria-controls={open ? popId : undefined} onClick={() => setOpen(!open)}>
        {variant === "pill" ? (
          <>
            <Shirt aria-hidden="true" />
            {list.length}
          </>
        ) : (
          tr("inventory.equipped.on", { count: list.length })
        )}
      </button>
      {open && (
        <div id={popId} className="inv-equip-pop" role="dialog" aria-label={label}>
          <ul>
            {list.map((gear, i) => (
              <li key={`${gear.name}-${i}`}>
                <span className="inv-equip-name">{gear.name}</span>
                <span className="inv-equip-cfg">{tr("inventory.equipped.config", { configs: gear.configs.join(", ") })}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </span>
  );
}
