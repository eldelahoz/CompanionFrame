import type { ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import { OVERLAY_DEFAULTS, type OverlayBox } from "../../overlays/common/overlayDefaults";
import type { OverlayKind } from "../../overlays/common/overlayTheme";

// Schematic 16:9 game screens on a 160x90 grid (fractions x160 / x90). The overlay box is
// OVERLAY_DEFAULTS; scan = what the scanner reads (reward_scan/image.rs FIXED_LAYOUTS, riven_scan/image.rs ROLL_CARD).
type Box = readonly [x: number, y: number, w: number, h: number];
interface Guide {
  scan?: Box;
  game: ReactNode;
}

const el = "fill-bg-hover stroke-text-muted";
const line = "fill-text-muted";
const bars = (x: number, y: number, ws: number[]) => ws.map((w, i) => <rect key={i} x={x} y={y + i * 3.5} width={w} height={1.6} rx={0.8} className={line} />);

export const GUIDES: Record<OverlayKind, Guide> = {
  reward: {
    scan: [39.2, 20.3, 80.5, 20.3],
    game: (
      <>
        {[39.2, 59.5, 79.8, 100.2].map((x) => (
          <g key={x}>
            <rect x={x} y={20.3} width={19.5} height={20.3} rx={1.2} className={el} strokeWidth={0.5} />
            <circle cx={x + 9.75} cy={28} r={4} className={line} />
            <rect x={x + 3} y={35} width={13.5} height={1.6} rx={0.8} className={line} />
          </g>
        ))}
        {bars(66, 80, [28])}
      </>
    ),
  },
  riven: {
    scan: [57.6, 37.4, 44.8, 39.1],
    game: (
      <>
        {bars(8, 8, [30, 22, 26])}
        <rect x={67} y={38} width={26} height={37} rx={1.5} className={el} strokeWidth={0.5} />
        <path d="M80 41 l4 4 -4 4 -4 -4z" className={line} />
        {bars(70, 55, [20, 18, 20, 14])}
        {[62, 82].map((x) => (
          <rect key={x} x={x} y={80} width={16} height={4} rx={1} className={el} strokeWidth={0.5} />
        ))}
      </>
    ),
  },
  arbi: {
    game: (
      <>
        {bars(6, 6, [34, 24, 28])}
        <path d="M80 42v6M77 45h6" className="stroke-text-muted" strokeWidth={0.6} />
        {bars(6, 74, [22, 22, 22, 22])}
        {[118, 127, 136, 145].map((x) => (
          <rect key={x} x={x} y={76} width={7} height={7} rx={1} className={el} strokeWidth={0.5} />
        ))}
      </>
    ),
  },
  trade: {
    game: (
      <>
        {[22, 84].map((x) => (
          <g key={x}>
            <rect x={x} y={14} width={54} height={50} rx={1.5} className={el} strokeWidth={0.5} />
            {bars(x + 4, 18, [24])}
            {[0, 1, 2, 3, 4, 5].map((i) => (
              <rect key={i} x={x + 4 + (i % 3) * 16} y={26 + Math.floor(i / 3) * 17} width={14} height={14} rx={1} className="fill-bg-raised" />
            ))}
          </g>
        ))}
        <rect x={68} y={68} width={24} height={6} rx={1.2} className={el} strokeWidth={0.5} />
      </>
    ),
  },
};

const rect = ([x, y, w, h]: Box) => ({ x, y, width: w, height: h });
const onScreen = ({ x, y, w, h }: OverlayBox): Box => [x * 160, y * 90, w * 160, h * 90];

/** Where an overlay goes over the game, as a schematic screen with a one-line caption. */
export default function OverlayGuide({ kind, tr, className = "" }: { kind: OverlayKind; tr: Translator; className?: string }) {
  const g = GUIDES[kind];
  return (
    <figure className={`m-0 flex flex-col gap-1.5 ${className}`} data-overlay-guide={kind}>
      <svg viewBox="0 0 160 90" role="img" aria-label={tr(`overlayGuide.${kind}.alt`)} className="block h-auto w-full rounded-[var(--radius-sm)] border border-border bg-bg-deep">
        {g.game}
        {g.scan && <rect {...rect(g.scan)} rx={1.5} className="fill-none stroke-accent" strokeWidth={0.8} strokeDasharray="2.5 1.5" />}
        <rect {...rect(onScreen(OVERLAY_DEFAULTS[kind]))} rx={1.5} className="fill-accent/30 stroke-accent" strokeWidth={1} />
      </svg>
      <figcaption className="text-xs leading-snug text-text-secondary">{tr(`overlayGuide.${kind}.caption`)}</figcaption>
    </figure>
  );
}
