import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import { overlayMonitors, overlayPlacementDetect, overlayPlacementGet, overlayPlacementReset, overlayPlacementSave, overlayPlacementSaveScale } from "../../lib/ipc";
import { OVERLAY_DEFAULTS, PLACEMENT_SCALE_MAX as SCALE_MAX, PLACEMENT_SCALE_MIN as SCALE_MIN, type OverlayBox } from "../../overlays/common/overlayDefaults";
import { Button } from "../ui";
import OverlayGuide from "./OverlayGuide";

interface Panel extends OverlayBox {
  kind: "reward" | "riven" | "arbi";
  title: MessageKey;
  text: MessageKey;
}

// The trade toast is fixed in place, so it has no stand-in here.
const PANELS: readonly Panel[] = [
  { kind: "reward", title: "setup.overlay.reward.title", text: "setup.overlay.reward.text", ...OVERLAY_DEFAULTS.reward },
  { kind: "riven", title: "setup.overlay.riven.title", text: "setup.overlay.riven.text", ...OVERLAY_DEFAULTS.riven },
  { kind: "arbi", title: "setup.overlay.arbiSummary.title", text: "setup.overlay.arbiSummary.text", ...OVERLAY_DEFAULTS.arbi },
];
const FALLBACK = { w: 1920, h: 1080 };
const KEY_STEP = 0.01;

type Pos = Record<string, { x: number; y: number; scale: number }>;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Drag stand-ins for the overlays over a miniature of the game screen; positions are saved per resolution. */
export default function OverlayPlacement({ tr }: { tr: Translator }) {
  const [res, setRes] = useState(FALLBACK);
  const [pos, setPos] = useState<Pos>({});
  const [selected, setSelected] = useState<Panel["kind"]>("reward");
  const [failed, setFailed] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  const drag = useRef<{ kind: Panel["kind"]; dx: number; dy: number } | null>(null);
  const alive = useRef(true);

  const load = useCallback(async () => {
    const rect = await overlayPlacementDetect().catch(() => null);
    const mon = rect ? null : (await overlayMonitors().catch(() => []))[0];
    const next = rect ? { w: rect.w, h: rect.h } : mon ? { w: mon.width, h: mon.height } : FALLBACK;
    const saved = await overlayPlacementGet(next.w, next.h).catch(() => []);
    if (!alive.current) return;
    setRes(next);
    setPos(
      Object.fromEntries(
        PANELS.map((p) => {
          const s = saved.find((e) => e.kind === p.kind)?.placement;
          return [p.kind, { x: s?.x_frac ?? p.x, y: s?.y_frac ?? p.y, scale: s?.scale ?? 1 }];
        }),
      ),
    );
  }, []);

  useEffect(() => {
    alive.current = true;
    void load();
    return () => {
      alive.current = false;
    };
  }, [load]);

  const size = (p: Panel) => ({ w: p.w * (pos[p.kind]?.scale ?? 1), h: p.h * (pos[p.kind]?.scale ?? 1) });
  const settle = (save: Promise<unknown>) => save.then(() => setFailed(false), () => setFailed(true));
  const place = (p: Panel, x: number, y: number) => {
    const s = size(p);
    const next = { x: clamp(x, 0, 1 - s.w), y: clamp(y, 0, 1 - s.h) };
    setPos((cur) => ({ ...cur, [p.kind]: { ...cur[p.kind], ...next } }));
    return next;
  };

  const down = (e: PointerEvent<HTMLDivElement>, p: Panel) => {
    const box = stage.current!.getBoundingClientRect();
    setSelected(p.kind);
    drag.current = { kind: p.kind, dx: (e.clientX - box.left) / box.width - pos[p.kind].x, dy: (e.clientY - box.top) / box.height - pos[p.kind].y };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const move = (e: PointerEvent<HTMLDivElement>, p: Panel) => {
    if (drag.current?.kind !== p.kind) return;
    const box = stage.current!.getBoundingClientRect();
    place(p, (e.clientX - box.left) / box.width - drag.current.dx, (e.clientY - box.top) / box.height - drag.current.dy);
  };
  const up = (p: Panel) => {
    if (drag.current?.kind !== p.kind) return;
    drag.current = null;
    void settle(overlayPlacementSave(res.w, res.h, p.kind, pos[p.kind].x, pos[p.kind].y));
  };
  const key = (e: KeyboardEvent<HTMLDivElement>, p: Panel) => {
    const d = { ArrowLeft: [-KEY_STEP, 0], ArrowRight: [KEY_STEP, 0], ArrowUp: [0, -KEY_STEP], ArrowDown: [0, KEY_STEP] }[e.key];
    if (!d) return;
    e.preventDefault();
    const n = place(p, pos[p.kind].x + d[0], pos[p.kind].y + d[1]);
    void settle(overlayPlacementSave(res.w, res.h, p.kind, n.x, n.y));
  };

  const active = PANELS.find((p) => p.kind === selected)!;
  const scale = pos[selected]?.scale ?? 1;
  // The position goes too: a scale-only entry would read back as the 0,0 corner.
  const saveScale = () =>
    void settle(overlayPlacementSave(res.w, res.h, selected, pos[selected].x, pos[selected].y).then(() => overlayPlacementSaveScale(res.w, res.h, selected, scale)));
  const reset = async () => {
    await overlayPlacementReset(res.w, res.h).catch(() => setFailed(true));
    await load();
  };

  return (
    <div className="flex flex-col gap-3" data-overlay-placement>
      <div ref={stage} className="relative mx-auto overflow-hidden rounded-[var(--radius-md)] border border-border bg-bg-base" style={{ aspectRatio: `${res.w} / ${res.h}`, width: `min(100%, ${(50 * res.w) / res.h}vh)` }}>
        <span className="absolute bottom-2 right-3 text-xs text-text-muted">
          {res.w} × {res.h}
        </span>
        {PANELS.map((p) => {
          const at = pos[p.kind];
          if (!at) return null;
          const s = size(p);
          return (
            <div
              key={p.kind}
              role="button"
              tabIndex={0}
              aria-label={tr(p.title)}
              aria-pressed={selected === p.kind}
              data-placement-panel={p.kind}
              onPointerDown={(e) => down(e, p)}
              onPointerMove={(e) => move(e, p)}
              onPointerUp={() => up(p)}
              onPointerCancel={() => up(p)}
              onKeyDown={(e) => key(e, p)}
              onFocus={() => setSelected(p.kind)}
              className={`absolute flex cursor-grab touch-none select-none flex-col justify-between rounded-[var(--radius-sm)] border bg-bg-raised p-2 text-xs shadow-[var(--shadow-2)] active:cursor-grabbing ${
                selected === p.kind ? "z-10 border-accent text-text-primary" : "border-border-strong text-text-secondary"
              }`}
              style={{ left: `${at.x * 100}%`, top: `${at.y * 100}%`, width: `${s.w * 100}%`, height: `${s.h * 100}%` }}
            >
              <span className="font-semibold leading-tight">{tr(p.title)}</span>
              <span className="text-text-muted">{tr("setup.overlay.dragMe")}</span>
            </div>
          );
        })}
      </div>
      <div className="flex flex-wrap items-start gap-4">
        <OverlayGuide kind={selected} tr={tr} className="w-64 max-w-full" />
        <p className="m-0 min-w-48 flex-1 text-sm text-text-secondary">{tr(active.text)}</p>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm text-text-secondary">
          {tr("setup.overlay.sizeLabel")}
          <input
            type="range"
            className="w-40 accent-accent"
            min={SCALE_MIN}
            max={SCALE_MAX}
            step={0.05}
            value={scale}
            onChange={(e) => {
              const v = Number(e.target.value);
              setPos((cur) => ({ ...cur, [selected]: { ...cur[selected], scale: v } }));
            }}
            // On release, like the app-size slider: one write per adjustment.
            onPointerUp={saveScale}
            onKeyUp={saveScale}
          />
          <span className="w-10 text-right text-accent">{Math.round(scale * 100)}%</span>
        </label>
        <Button size="sm" variant="ghost" onClick={() => void reset()}>
          {tr("setup.overlay.reset")}
        </Button>
      </div>
      <p className="m-0 text-xs text-text-muted">{tr("setup.overlay.hint")}</p>
      {failed && (
        <p className="m-0 text-xs text-danger" role="alert">
          {tr("setup.overlay.saveFailed")}
        </p>
      )}
    </div>
  );
}
