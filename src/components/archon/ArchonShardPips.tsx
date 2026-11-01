import { useState } from "react";
import type { Translator } from "../../lib/i18n";
import { archonShardColorKey, archonShardIconUrl, type ArchonShardColor, type ArchonShardSlot } from "../../lib/inventory/archonShards";
import type { ItemDbEntry } from "../../types/inventory";

// Shard hues are fixed game colours, so they stay out of the theme; only the fallback dot needs them.
const SHARD_HEX: Record<ArchonShardColor, string> = {
  crimson: "#e2465b",
  amber: "#e8a63a",
  azure: "#3f9ee0",
  emerald: "#3fc489",
  topaz: "#ef8a3c",
  violet: "#a271e6",
};
const SIZE = { sm: "h-2 w-2", md: "h-3 w-3", lg: "h-3.5 w-3.5" } as const;

interface Props {
  slots: ArchonShardSlot[];
  itemDb: Record<string, ItemDbEntry>;
  tr: Translator;
  /** Render unfilled sockets as placeholders instead of dropping them. */
  showEmpty?: boolean;
  size?: keyof typeof SIZE;
  title?: string;
}

export default function ArchonShardPips({ slots, itemDb, tr, showEmpty = false, size = "sm", title }: Props) {
  // A 404 on the icon would otherwise leave an invisible pip.
  const [broken, setBroken] = useState<string[]>([]);
  const shown = showEmpty ? slots : slots.filter((s) => s.filled);
  if (shown.length === 0) return null;
  return (
    <span className="inline-flex items-center gap-0.5" title={title} data-archon-pips>
      {shown.map((slot) => {
        const icon = archonShardIconUrl(itemDb, slot.color, slot.tauforged);
        const label = slot.filled ? (slot.color ? tr(archonShardColorKey(slot.color)) : tr("common.unknown")) + (slot.tauforged ? ` - ${tr("archon.tauforged")}` : "") : undefined;
        return icon && !broken.includes(icon) ? (
          <img
            key={slot.index}
            className={`${SIZE[size]} shrink-0 object-contain`}
            src={icon}
            alt=""
            loading="lazy"
            title={label}
            data-archon-pip={slot.color}
            data-archon-tau={slot.tauforged || undefined}
            onError={() => setBroken((b) => [...b, icon])}
          />
        ) : (
          <span
            key={slot.index}
            className={`${SIZE[size]} shrink-0 rounded-full border ${slot.filled ? "border-transparent" : "border-border-subtle"} ${slot.tauforged ? "ring-1 ring-warning" : ""}`}
            style={{ background: slot.color ? SHARD_HEX[slot.color] : slot.filled ? "var(--text-muted)" : "transparent" }}
            title={label}
            data-archon-pip={slot.color ?? (slot.filled ? "unknown" : "empty")}
            data-archon-tau={slot.tauforged || undefined}
          />
        );
      })}
    </span>
  );
}
