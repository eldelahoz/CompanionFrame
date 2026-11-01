import { GripVertical } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties, type PointerEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { CHIP_PX, chipBelow, frameTop, OVERLAY_LAYOUT_RESET_EVENT, type Box, type OverlayContext } from "./OverlayShell";
import type { OverlayKind } from "./overlayTheme";

const SAVE_DELAY_MS = 300;
const POLL_MS = 250;
const SCALE_MIN = 0.6;
const SCALE_MAX = 1.6;
const NO_DRAG = "button, a, input, select, [data-no-drag]";
const parse = (json: string | null): unknown => {
  try {
    return JSON.parse(json ?? "null");
  } catch {
    return null;
  }
};

/**
 * A panel inside a shell that keeps its own offset and scale (layout keys `offset:<id>` / `scale:<id>`),
 * independent of the shell's main panel. Drag it with the mouse in move or interaction mode; in move mode it
 * gets its own frame with a resize corner. The child must be the one positioned element; it reads --pl-x/y/scale.
 */
export function MovablePanel({ id, kind, ctx, children }: { id: string; kind: OverlayKind; ctx: OverlayContext; children: ReactNode }) {
  const { ipc, tr, moving, interactive, offset } = ctx;
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const [scale, setScale] = useState(1);
  const [box, setBox] = useState<Box | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const timers = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const live = useRef({ pos, scale });
  live.current = { pos, scale };
  const offsetKey = `offset:${id}`;
  const scaleKey = `scale:${id}`;

  const save = (key: string, value: unknown) => {
    clearTimeout(timers.current[key]);
    timers.current[key] = setTimeout(() => void ipc.layout.save(key, JSON.stringify(value)).catch(() => {}), SAVE_DELAY_MS);
  };
  useEffect(() => () => Object.values(timers.current).forEach(clearTimeout), []);

  useEffect(() => {
    let alive = true;
    void ipc.layout.get(offsetKey).catch(() => null).then((j) => {
      const p = parse(j) as { x?: number; y?: number } | null;
      if (alive && Number.isFinite(p?.x) && Number.isFinite(p?.y)) setPos({ x: p!.x!, y: p!.y! });
    });
    void ipc.layout.get(scaleKey).catch(() => null).then((j) => {
      const s = parse(j);
      if (alive && typeof s === "number" && Number.isFinite(s)) setScale(Math.min(SCALE_MAX, Math.max(SCALE_MIN, s)));
    });
    const off = ipc.on<string>(OVERLAY_LAYOUT_RESET_EVENT, (key) => {
      if (key === offsetKey) setPos({ x: 0, y: 0 });
      if (key === scaleKey) setScale(1);
    });
    return () => ((alive = false), off());
  }, [ipc, offsetKey, scaleKey]);

  const panelRect = () => ref.current?.firstElementChild?.getBoundingClientRect();
  // Where the panel is drawn, for the move frame (it moves and resizes under the mouse).
  useEffect(() => {
    if (!moving) return setBox(null);
    const measure = () => {
      const r = panelRect();
      const next = r && r.width > 0 ? { left: r.left, top: r.top, right: r.right, bottom: r.bottom } : null;
      setBox((old) => (old && next && old.left === next.left && old.top === next.top && old.right === next.right && old.bottom === next.bottom ? old : next));
    };
    measure();
    const t = setInterval(measure, POLL_MS);
    return () => clearInterval(t);
  }, [moving, pos, scale]);

  // A press on the panel drags it, not the shell's main panel: capture runs before the shell's handler.
  useEffect(() => {
    if (!interactive && !moving) return;
    let drag: { x: number; y: number } | null = null;
    const end = () => {
      if (!drag) return;
      drag = null;
      if (moving) void ipc.move?.interactive(kind, false).catch(() => {});
    };
    const onDown = (e: MouseEvent) => {
      const r = panelRect();
      if (!r || (e.button !== 0 && e.button !== 2)) return;
      const onChip = e.target instanceof Element && !!e.target.closest(`[data-own-frame="${id}"]`);
      if (!onChip && (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom)) return;
      if (e.button === 0 && e.target instanceof Element && e.target.closest(NO_DRAG)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      drag = { x: e.screenX, y: e.screenY };
      if (moving) void ipc.move?.interactive(kind, true).catch(() => {});
    };
    const onMove = (e: MouseEvent) => {
      if (!drag) return;
      if ((e.buttons & 3) === 0) return end();
      const next = { x: live.current.pos.x + e.screenX - drag.x, y: live.current.pos.y + e.screenY - drag.y };
      drag = { x: e.screenX, y: e.screenY };
      live.current.pos = next;
      setPos(next);
      save(offsetKey, next);
    };
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", end);
    window.addEventListener("blur", end);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", end);
      window.removeEventListener("blur", end);
      end();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [interactive, moving, ipc, kind, offsetKey, id]);

  // Corner handle: dragging it by dx grows the panel by dx, so the scale follows the width ratio.
  const resize = useRef<{ x: number; w: number; scale: number } | null>(null);
  const onResizeDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !box) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    resize.current = { x: e.screenX, w: box.right - box.left, scale };
    void ipc.move?.interactive(kind, true).catch(() => {});
  };
  const onResizeMove = (e: PointerEvent<HTMLDivElement>) => {
    const r = resize.current;
    if (!r) return;
    const next = Math.round(Math.min(SCALE_MAX, Math.max(SCALE_MIN, r.scale * (1 + (e.screenX - r.x) / r.w))) * 100) / 100;
    setScale(next);
    save(scaleKey, next);
  };
  const onResizeEnd = () => {
    if (!resize.current) return;
    resize.current = null;
    void ipc.move?.interactive(kind, false).catch(() => {});
  };

  const style = { display: "contents", "--pl-x": `${pos.x - (offset?.x ?? 0)}px`, "--pl-y": `${pos.y - (offset?.y ?? 0)}px`, "--pl-scale": scale } as CSSProperties;
  return (
    <>
      <div ref={ref} data-own-panel={id} style={style}>
        {children}
      </div>
      {moving &&
        box &&
        createPortal(
          <div
            className="overlay-move-frame"
            data-overlay-move
            data-own-frame={id}
            data-chip-below={chipBelow(box) || undefined}
            style={{ left: box.left - 4, top: frameTop(box), width: box.right - box.left + 8, height: box.bottom - box.top + CHIP_PX + 4 }}
          >
            <div className="overlay-move-chip">
              <GripVertical className="ov-icon" aria-hidden />
              <span>{tr("overlay.move.drag")}</span>
            </div>
            <div className="overlay-move-resize" data-no-drag title={tr("overlay.move.resize")} onPointerDown={onResizeDown} onPointerMove={onResizeMove} onPointerUp={onResizeEnd} onPointerCancel={onResizeEnd} />
          </div>,
          document.body,
        )}
    </>
  );
}
