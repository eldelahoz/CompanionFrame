import { formatInt } from "../../lib/format";
import { hueColor } from "../../lib/arbi/arbiChartData";
import { useMemo } from "react";
import { applyFloorFilter, drawnSpawnPoints, placeSpawnPoints, resolveMinimap, SPAWN_ELEVATION_COLORS, type ArbiFloorLabel } from "../../lib/arbi/arbiMinimap";
import { computeSpawnMap } from "../../lib/arbi/arbiSpawnMap";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { ArbiRun, ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";

/** One note per floor name; a new name in the mirrored catalog fails here. */
const FLOOR_NOTE_KEYS: Record<ArbiFloorLabel, MessageKey> = { bottom: "arbi.spawnMap.bottomFloor" };

/** Below this the count would spill out of the circle, so the hover title carries it. */
const LABEL_MIN_RADIUS = 3;
/** Bubble radius in the 1000px tile viewBox: 8px floor, 13px of growth. */
const TILE_MIN_RADIUS = 8;
const TILE_RADIUS_SPAN = 13;
const TILE_RING_WIDTH = 2;
const TILE_FONT_SIZE = 16;

const round2 = (v: number) => Math.round(v * 100) / 100;
const bubbleColor = hueColor;
/** Dark shell ink: readable on the bright fill under every theme. */
const labelColor = () => "var(--bg-deep)";
const formatCount = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(1));

export default function ArbiSpawnMap({ stats, run, tr }: { stats: ArbiRunStats; run: ArbiRun; tr: Translator }) {
  const { minimap, floor, map, view } = useMemo(() => {
    const minimap = resolveMinimap(run, stats.spawnPoints);
    const floor = minimap ? applyFloorFilter(minimap, stats.spawnPoints) : null;
    // Stats, map and the side list all describe the set the map draws.
    const map = computeSpawnMap(floor ? drawnSpawnPoints(floor) : stats.spawnPoints);
    if (!map) return { minimap, floor, map, view: null };
    // Both modes feed one loop: without a tile the bubbles keep their own square viewBox.
    if (!minimap || !floor) {
      return {
        minimap,
        floor,
        map,
        view: {
          viewBox: `0 0 ${map.viewSize} ${map.viewSize}`,
          imageUrl: null as string | null,
          imageWidth: map.viewSize,
          imageHeight: map.viewSize,
          strokeWidth: 0.4,
          fontSize: 2.4,
          tooltipKey: "arbi.spawnMap.point" as MessageKey,
          bubbles: map.bubbles.map((b, i) => ({ ...b, fillOpacity: 0.65, stroke: bubbleColor(b.hue), level: 0, showLabel: i < map.top.length && b.r >= LABEL_MIN_RADIUS })),
        },
      };
    }
    const placement = placeSpawnPoints(minimap, floor.points);
    return {
      minimap,
      floor,
      map,
      view: {
        viewBox: `0 0 ${minimap.width} ${minimap.height}`,
        imageUrl: minimap.imageUrl as string | null,
        imageWidth: minimap.width,
        imageHeight: minimap.height,
        strokeWidth: TILE_RING_WIDTH,
        fontSize: TILE_FONT_SIZE,
        tooltipKey: "arbi.spawnMap.pointElevation" as MessageKey,
        bubbles: map.bubbles.flatMap((b) => {
          const pos = placement.get(b.id);
          // A point with no reference position sits on another floor of the tile.
          if (!pos || !floor.matched.has(b.id)) return [];
          return [
            {
              ...b,
              cx: pos.cx,
              cy: pos.cy,
              r: round2(TILE_MIN_RADIUS + Math.sqrt(b.count / map.maxCount) * TILE_RADIUS_SPAN),
              fillOpacity: round2(0.22 + (b.hue / 120) * 0.5),
              stroke: SPAWN_ELEVATION_COLORS[pos.level],
              level: pos.level + 1,
              // Four closet points overlap on the map, so their labels would too.
              showLabel: false,
            },
          ];
        }),
      },
    };
  }, [stats, run]);

  if (!map || !view) return null;
  const busiest = map.top[0];
  const floorNoteKey = floor?.floorLabel ? FLOOR_NOTE_KEYS[floor.floorLabel] : null;
  const tiles: Array<{ key: string; label: MessageKey; value: string; sub: string }> = [
    { key: "points", label: "arbi.spawnMap.pointsFired", value: formatInt(map.pointCount), sub: "" },
    { key: "spawns", label: "arbi.spawnMap.loggedSpawns", value: formatInt(map.totalSpawns), sub: "" },
    { key: "busiest", label: "arbi.spawnMap.busiest", value: busiest ? `#${busiest.label}` : "–", sub: busiest ? `${busiest.count}, ${busiest.sharePct.toFixed(1)}%` : "" },
    { key: "avg", label: "arbi.spawnMap.avgPerPoint", value: map.avgPerPoint.toFixed(1), sub: tr("arbi.spawnMap.median", { value: formatCount(map.medianCount) }) },
    { key: "topShare", label: "arbi.spawnMap.topShare", value: `${map.topSharePct.toFixed(0)}%`, sub: "" },
    { key: "cold", label: "arbi.spawnMap.coldPoints", value: formatInt(map.coldPoints), sub: tr("arbi.spawnMap.coldDesc", { count: String(map.coldMaxCount) }) },
  ];

  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr("arbi.spawnMap.title")} />
      <p className="mb-3 mt-1 text-xs text-text-muted">
        {tr("arbi.spawnMap.desc")}
        {floorNoteKey && <span className="ml-1">{tr(floorNoteKey)}</span>}
      </p>
      <div className="mb-4 grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6" data-arbi-spawn-stats>
        {tiles.map((t) => (
          <div key={t.key} className="flex min-w-0 flex-col gap-1 rounded-[var(--radius-sm)] border border-border/60 bg-bg-raised/40 px-2.5 py-2">
            <span className="truncate text-xs font-semibold text-text-muted">{tr(t.label)}</span>
            <span className="truncate font-mono text-lg font-bold leading-none text-text-primary">{t.value}</span>
            <span className="h-3.5 truncate text-xs leading-none text-text-muted">{t.sub}</span>
          </div>
        ))}
      </div>
      <div className="grid gap-4 md:grid-cols-2" data-arbi-spawn-map>
        <svg
          className="aspect-square w-full rounded-[var(--radius-md)] border border-border bg-bg-raised"
          viewBox={view.viewBox}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={tr("arbi.spawnMap.title")}
          data-arbi-minimap={minimap?.key}
        >
          {view.imageUrl && <image href={view.imageUrl} x="0" y="0" width={view.imageWidth} height={view.imageHeight} preserveAspectRatio="xMidYMid meet" />}
          {view.bubbles.map((b) => (
            <g key={b.id}>
              <circle cx={b.cx} cy={b.cy} r={b.r} fill={bubbleColor(b.hue)} fillOpacity={b.fillOpacity} stroke={b.stroke} strokeWidth={view.strokeWidth}>
                <title>{tr(view.tooltipKey, { id: b.label, count: String(b.count), level: String(b.level) })}</title>
              </circle>
              {b.showLabel && (
                <text x={b.cx} y={b.cy} textAnchor="middle" dominantBaseline="central" fontSize={view.fontSize} className="pointer-events-none font-mono font-bold" fill={labelColor()}>
                  {b.count}
                </text>
              )}
            </g>
          ))}
        </svg>
        <div className="flex flex-col">
          <h4 className="m-0 text-xs font-semibold text-text-secondary">{tr("arbi.spawnMap.mostActive")}</h4>
          <ul className="m-0 mt-2 flex list-none flex-col gap-1 p-0">
            {map.top.map((b) => (
              <li key={b.id} className="flex items-center gap-2 text-xs">
                <span className="w-10 shrink-0 font-mono text-text-secondary">#{b.label}</span>
                <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-bg-raised">
                  <span className="block h-full rounded-full" style={{ width: `${(b.count / map.maxCount) * 100}%`, backgroundColor: bubbleColor(b.hue) }} />
                </span>
                <span className="w-10 shrink-0 text-right font-mono font-semibold text-text-primary">{b.count}</span>
                <span className="w-12 shrink-0 text-right font-mono text-text-muted">{b.sharePct.toFixed(1)}%</span>
              </li>
            ))}
          </ul>
        </div>
      </div>
      <p className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-muted">
        <span>{tr("arbi.spawnMap.legend")}</span>
        {minimap && (
          <>
            <span className="flex items-center gap-1.5">
              <span className="font-semibold">{tr("arbi.spawnMap.elevation")}</span>
              <span>{tr("arbi.spawnMap.elevationLow")}</span>
              <span className="flex items-center gap-0.5">
                {SPAWN_ELEVATION_COLORS.map((c) => (
                  <span key={c} className="inline-block h-2 w-3 rounded-[2px]" style={{ backgroundColor: c }} />
                ))}
              </span>
              <span>{tr("arbi.spawnMap.elevationHigh")}</span>
            </span>
            <span>{tr("arbi.spawnMap.credit")}</span>
          </>
        )}
      </p>
    </Panel>
  );
}
