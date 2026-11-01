import { Bell, BellOff } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import { GameIcon, type GameIconName } from "../icons";
import { IconButton } from "../ui";

// Static class strings so Tailwind can see them; the tint is the state colour at 10%.
const STATE_CLASS: Record<string, string> = {
  day: "text-[var(--world-state-day-text)] bg-[color-mix(in_srgb,var(--world-state-day-text)_10%,transparent)]",
  night: "text-[var(--world-state-night-text)] bg-[color-mix(in_srgb,var(--world-state-night-text)_10%,transparent)]",
  warm: "text-[var(--world-state-warm-text)] bg-[color-mix(in_srgb,var(--world-state-warm-text)_10%,transparent)]",
  cold: "text-[var(--world-state-cold-text)] bg-[color-mix(in_srgb,var(--world-state-cold-text)_10%,transparent)]",
  fass: "text-[var(--world-state-fass-text)] bg-[color-mix(in_srgb,var(--world-state-fass-text)_10%,transparent)]",
  vome: "text-[var(--world-state-vome-text)] bg-[color-mix(in_srgb,var(--world-state-vome-text)_10%,transparent)]",
  anger: "text-[var(--world-state-anger-text)] bg-[color-mix(in_srgb,var(--world-state-anger-text)_10%,transparent)]",
  joy: "text-[var(--world-state-joy-text)] bg-[color-mix(in_srgb,var(--world-state-joy-text)_10%,transparent)]",
  envy: "text-[var(--world-state-envy-text)] bg-[color-mix(in_srgb,var(--world-state-envy-text)_10%,transparent)]",
  sorrow: "text-[var(--world-state-sorrow-text)] bg-[color-mix(in_srgb,var(--world-state-sorrow-text)_10%,transparent)]",
  fear: "text-[var(--world-state-fear-text)] bg-[color-mix(in_srgb,var(--world-state-fear-text)_10%,transparent)]",
};

export const TIMER_CLASS = "text-sm tabular-nums whitespace-nowrap";
export const timerColor = (urgent: boolean) => (urgent ? "text-[var(--world-timer-urgent-text)]" : "text-text-primary");

interface Props {
  name: string;
  /** Cycle key; picks the bundled world art. */
  planet?: string;
  /** Explicit art for callers that bring their own; `planet` wins. */
  stateLabel: string;
  stateClass: string;
  nextLabel: string;
  time: string;
  urgent: boolean;
  tr: Translator;
  /** Bell next to the timer: notify when this cycle changes. */
  bell?: { on: boolean; title: string; onToggle: () => void };
}

/** Planet-cycle row: icon, state badge, time remaining. */
const WORLD_ICON: Record<string, GameIconName> = { earth: "world-earth", cetus: "world-earth", vallis: "world-vallis", cambion: "world-cambion", duviri: "world-zariman" };

export default function CycleRow({ name, planet, stateLabel, stateClass, nextLabel, time, urgent, tr, bell }: Props) {
  const art = planet ? WORLD_ICON[planet] : undefined;
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-border py-2 last:border-b-0" data-cycle-row={name}>
      <div className="flex min-w-0 items-center gap-2">
        {art ? <GameIcon name={art} className="h-8 w-8 shrink-0 rounded-full" /> : null}
        <span className="whitespace-nowrap text-sm font-semibold text-text-primary">{name}</span>
        <span className={`whitespace-nowrap rounded-[var(--radius-sm)] px-1.5 py-0.5 text-xs font-semibold ${STATE_CLASS[stateClass] ?? ""}`}>{stateLabel}</span>
      </div>
      <span className="flex shrink-0 items-center gap-2">
        <span className="whitespace-nowrap text-xs text-text-muted">{tr("world.nextLabelIn", { nextLabel })}</span>
        <span className={`${TIMER_CLASS} ${timerColor(urgent)}`}>{time}</span>
        {bell && (
          <IconButton label={bell.title} aria-pressed={bell.on} className={bell.on ? "!text-accent" : ""} onClick={bell.onToggle}>
            {bell.on ? <Bell aria-hidden="true" /> : <BellOff aria-hidden="true" />}
          </IconButton>
        )}
      </span>
    </div>
  );
}
