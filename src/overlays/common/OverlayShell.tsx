import { GripVertical } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import { OVERLAY_DEFAULTS } from "./overlayDefaults";
import { installOverlayDrag } from "./overlayDrag";
import type { OverlayIpc } from "./overlayIpc";
import { applyOverlayThemeVars, settingsThemeVars, type OverlayKind } from "./overlayTheme";
import { useOverlayI18n } from "./useOverlayI18n";

export const OVERLAY_THEME_EVENT = "overlay-theme-vars";
/** Rust emits these after the settings form saves / a layout is reset (see services/overlay). */
export const OVERLAY_SETTINGS_EVENT = "overlay-settings";
export const OVERLAY_LAYOUT_RESET_EVENT = "overlay-layout-reset";
export const OVERLAY_INTERACTION_EVENT = "overlay-interaction-mode";
/** `true` while the overlays are being moved (`services/overlay::set_move_mode`). */
export const OVERLAY_MOVE_EVENT = "overlay-move-mode";

export interface OverlayContext {
  tr: Translator;
  ipc: OverlayIpc;
  interactive: boolean;
  /** Move mode: screens draw sample content when they have nothing live, and take no clicks. */
  moving?: boolean;
  /** The shell's own drag offset, for a panel that keeps a position of its own (MovablePanel). */
  offset?: { x: number; y: number };
}

export type Box = { left: number; top: number; right: number; bottom: number };
const MOVE_POLL_MS = 250;
/** Room for the move chip above the panel, and slack so a quick drag stays inside the region. */
export const CHIP_PX = 30;
const SLACK_PX = 24;
const INTERACTIVE_SELECTOR = 'a[href], button:not(:disabled), [role=button], input, select, textarea, [data-overlay-interactive]';
const CLICK_PAD_PX = 4;
const REGION_DEBOUNCE_MS = 100;

/** Union of the rendered boxes inside `root` (the screen's panels), in window CSS px. */
function contentBox(root: HTMLElement): Box | null {
  let box: Box | null = null;
  for (const el of root.querySelectorAll<HTMLElement>("[data-overlay-content] *")) {
    if (el.closest("[data-own-panel]")) continue;
    const r = el.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    box = box
      ? { left: Math.min(box.left, r.left), top: Math.min(box.top, r.top), right: Math.max(box.right, r.right), bottom: Math.max(box.bottom, r.bottom) }
      : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  }
  return box;
}

/** A panel at the screen top (the reward row) has no room for the tab above it, so the tab goes below. */
export const chipBelow = (box: Box) => box.top < CHIP_PX;
/** Top edge of the frame: the tab sits above the panel, or the frame starts at the panel when the tab is below. */
export const frameTop = (box: Box) => (chipBelow(box) ? box.top - 4 : box.top - CHIP_PX);

const sameBox = (a: Box | null, b: Box | null) => a === b || (!!a && !!b && a.left === b.left && a.top === b.top && a.right === b.right && a.bottom === b.bottom);

/** Panels that move on their own (MovablePanel): the shell only has to keep their area clickable. */
function ownPanelBoxes(root: HTMLElement): Box[] {
  const boxes: Box[] = [];
  for (const el of root.querySelectorAll<HTMLElement>("[data-own-panel] > *")) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) boxes.push({ left: r.left, top: r.top, right: r.right, bottom: r.bottom });
  }
  return boxes;
}
const sameBoxes = (a: Box[], b: Box[]) => a.length === b.length && a.every((x, i) => sameBox(x, b[i]));

type Pos = { x: number; y: number };
type Base = { x: number; y: number; scale: number };
const posKey = (kind: OverlayKind) => `offset:${kind}`;
const SAVE_DELAY_MS = 300;
/** Same range the backend accepts (placement.rs); the trade toast has a fixed size. */
const SCALE_MIN = 0.6;
const SCALE_MAX = 1.6;
const clampScale = (v: number) => Math.min(SCALE_MAX, Math.max(SCALE_MIN, v));

function parsePos(json: string | null): Pos {
  try {
    const p = JSON.parse(json ?? "null");
    if (Number.isFinite(p?.x) && Number.isFinite(p?.y)) return { x: p.x, y: p.y };
  } catch {}
  return { x: 0, y: 0 };
}

/**
 * Common frame of every overlay: theme + opacity, i18n gate, interaction mode and a draggable offset.
 * Dragging only moves the content (CSS translate) and persists it in the Rust store; the native window is never touched.
 */
export function OverlayShell({
  kind,
  ipc,
  children,
}: {
  kind: OverlayKind;
  ipc: OverlayIpc;
  children: (ctx: OverlayContext) => ReactNode;
}) {
  const { tr, lang, ready } = useOverlayI18n(ipc);
  const [interactive, setInteractive] = useState(false);
  const [pos, setPos] = useState<Pos | null>(null);
  const [base, setBase] = useState<Base>({ ...OVERLAY_DEFAULTS[kind], scale: 1 });
  const saveTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    const root = document.documentElement;
    applyOverlayThemeVars(root, {}, kind);
    const load = () =>
      ipc
        .settings()
        .then((s) => {
          root.dataset.theme = s.theme;
          applyOverlayThemeVars(root, settingsThemeVars(s, kind), kind);
        })
        .catch(() => {});
    void load();
    const offTheme = ipc.on<Record<string, unknown>>(OVERLAY_THEME_EVENT, (vars) => applyOverlayThemeVars(root, vars, kind));
    const offSettings = ipc.on<null>(OVERLAY_SETTINGS_EVENT, load);
    return () => (offTheme(), offSettings());
  }, [ipc, kind]);

  useEffect(() => void (document.documentElement.lang = lang), [lang]);
  useEffect(
    () => ipc.on<boolean | { interactive?: boolean }>(OVERLAY_INTERACTION_EVENT, (p) => setInteractive(typeof p === "boolean" ? p : !!p?.interactive)),
    [ipc],
  );

  useEffect(() => {
    let live = true;
    ipc.layout
      .get(posKey(kind))
      .catch(() => null)
      .then((json) => live && setPos(parsePos(json)));
    return () => {
      live = false;
      clearTimeout(saveTimer.current);
    };
  }, [ipc, kind]);

  // Setup's saved spot for this monitor size, else the default; the window spans the monitor, so its size is the key.
  useEffect(() => {
    let live = true;
    // Only the newest request may set the spot: an older response can land after a newer one.
    let latest = 0;
    const load = () => {
      const req = ++latest;
      return ipc
        .placement(Math.round(window.innerWidth * devicePixelRatio), Math.round(window.innerHeight * devicePixelRatio))
        .catch(() => [])
        .then((saved) => {
          const p = saved.find((e) => e.kind === kind)?.placement;
          const d = OVERLAY_DEFAULTS[kind];
          if (live && req === latest) setBase({ x: p?.x_frac ?? d.x, y: p?.y_frac ?? d.y, scale: p?.scale ?? 1 });
        });
    };
    void load();
    const off = ipc.on<null>(OVERLAY_SETTINGS_EVENT, load);
    // The page mounts in the hidden default-size window; it gets the monitor size only when shown.
    const onResize = () => void load();
    window.addEventListener("resize", onResize);
    return () => ((live = false), off(), window.removeEventListener("resize", onResize));
  }, [ipc, kind]);

  useEffect(
    () => ipc.on<string>(OVERLAY_LAYOUT_RESET_EVENT, (key) => key === posKey(kind) && setPos({ x: 0, y: 0 })),
    [ipc, kind],
  );

  // `?move=1` starts in move mode, for browser previews and screenshots (the backend toggles it in the app).
  const [moving, setMoving] = useState(() => new URLSearchParams(window.location.search).get("move") === "1");
  const [box, setBox] = useState<Box | null>(null);
  const [extra, setExtra] = useState<Box[]>([]);
  const shellRef = useRef<HTMLDivElement>(null);
  useEffect(() => ipc.on<boolean>(OVERLAY_MOVE_EVENT, (on) => setMoving(!!on)), [ipc]);
  const done = () => void ipc.move?.done().catch(() => {});

  // While moving, track where the panel is drawn (samples render after i18n, drags move it).
  useEffect(() => {
    if (!moving) return setBox(null), setExtra([]);
    const measure = () => {
      const next = shellRef.current ? contentBox(shellRef.current) : null;
      setBox((old) => (sameBox(old, next) ? old : next));
      const own = shellRef.current ? ownPanelBoxes(shellRef.current) : [];
      setExtra((old) => (sameBoxes(old, own) ? old : own));
    };
    measure();
    const t = setInterval(measure, MOVE_POLL_MS);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && done();
    window.addEventListener("keydown", onKey);
    return () => (clearInterval(t), window.removeEventListener("keydown", onKey));
  }, [moving, pos, ipc]);

  // Only the panel (and its chip) takes clicks: the four overlay windows span the same monitor.
  useEffect(() => {
    const boxes = box ? [box, ...extra] : extra;
    if (!moving || !boxes.length) return;
    const d = window.devicePixelRatio || 1;
    void ipc.move
      ?.regions(
        kind,
        boxes.map((b) => ({
          x: Math.round((window.screenX + b.left - SLACK_PX) * d),
          y: Math.round((window.screenY + frameTop(b) - SLACK_PX) * d),
          w: Math.round((b.right - b.left + 2 * SLACK_PX) * d),
          h: Math.round((b.bottom - b.top + CHIP_PX + 2 * SLACK_PX) * d),
        })),
      )
      .catch(() => {});
  }, [moving, box, extra, ipc, kind]);

  // Outside move mode only the buttons and links take clicks; the rest stays click-through to the game.
  // The trade toast reports its own box, so it is left alone.
  useEffect(() => {
    const root = shellRef.current;
    if (moving || kind === "trade" || !root || !ipc.move) return;
    let last = "[]";
    const report = () => {
      const d = window.devicePixelRatio || 1;
      const els = document.hidden ? [] : [...root.querySelectorAll<HTMLElement>(INTERACTIVE_SELECTOR)];
      const rects = els.flatMap((el) => {
        const r = el.getBoundingClientRect();
        if (r.width <= 0 || r.height <= 0) return [];
        return [
          {
            x: Math.round((window.screenX + r.left - CLICK_PAD_PX) * d),
            y: Math.round((window.screenY + r.top - CLICK_PAD_PX) * d),
            w: Math.round((r.width + 2 * CLICK_PAD_PX) * d),
            h: Math.round((r.height + 2 * CLICK_PAD_PX) * d),
          },
        ];
      });
      const key = JSON.stringify(rects);
      if (key === last) return;
      last = key;
      void ipc.move?.regions(kind, rects).catch(() => {});
    };
    let t: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => (clearTimeout(t), (t = setTimeout(report, REGION_DEBOUNCE_MS)));
    const mo = new MutationObserver(schedule);
    mo.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    ro?.observe(root);
    window.addEventListener("resize", schedule);
    document.addEventListener("visibilitychange", schedule);
    report();
    return () => {
      clearTimeout(t);
      mo.disconnect();
      ro?.disconnect();
      window.removeEventListener("resize", schedule);
      document.removeEventListener("visibilitychange", schedule);
      if (last !== "[]") void ipc.move?.regions(kind, []).catch(() => {});
    };
  }, [moving, kind, ipc, ready, pos]);

  // Corner handle: dragging it by dx grows the panel by dx, so the scale follows the width ratio.
  const resize = useRef<{ x: number; w: number; scale: number } | null>(null);
  const scaleTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const onResizeDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !box) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    resize.current = { x: e.screenX, w: box.right - box.left, scale: base.scale };
    void ipc.move?.interactive(kind, true).catch(() => {});
  };
  const onResizeMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const r = resize.current;
    if (!r) return;
    const scale = Math.round(clampScale(r.scale * (1 + (e.screenX - r.x) / r.w)) * 100) / 100;
    setBase((b) => ({ ...b, scale }));
    clearTimeout(scaleTimer.current);
    scaleTimer.current = setTimeout(
      () => void ipc.saveScale?.(Math.round(window.innerWidth * devicePixelRatio), Math.round(window.innerHeight * devicePixelRatio), kind, scale).catch(() => {}),
      SAVE_DELAY_MS,
    );
  };
  const onResizeEnd = () => {
    if (!resize.current) return;
    resize.current = null;
    void ipc.move?.interactive(kind, false).catch(() => {});
  };
  useEffect(() => () => clearTimeout(scaleTimer.current), []);

  useEffect(() => {
    if (!interactive && !moving) return;
    return installOverlayDrag({
      isInteractive: () => true,
      // The whole window takes the mouse during a drag, so a fast move never leaves the region.
      onDrag: (on) => void (moving && ipc.move?.interactive(kind, on).catch(() => {})),
      moveBy: (dx, dy) =>
        setPos((p) => {
          const next = { x: (p?.x ?? 0) + dx, y: (p?.y ?? 0) + dy };
          clearTimeout(saveTimer.current);
          saveTimer.current = setTimeout(() => void ipc.layout.save(posKey(kind), JSON.stringify(next)).catch(() => {}), SAVE_DELAY_MS);
          return next;
        }),
    });
  }, [interactive, moving, kind, ipc]);

  if (!ready || !pos) return null;
  return (
    <>
    {moving && box && (
      <div className="overlay-move-frame" data-overlay-move data-chip-below={chipBelow(box) || undefined} style={{ left: box.left - 4, top: frameTop(box), width: box.right - box.left + 8, height: box.bottom - box.top + CHIP_PX + 4 }}>
        <div className="overlay-move-chip">
          <GripVertical className="ov-icon" aria-hidden />
          <span>{tr("overlay.move.drag")}</span>
          <button type="button" onClick={done}>
            {tr("overlay.move.done")}
          </button>
        </div>
        {kind !== "trade" && (
          <div
            className="overlay-move-resize"
            data-no-drag
            title={tr("overlay.move.resize")}
            onPointerDown={onResizeDown}
            onPointerMove={onResizeMove}
            onPointerUp={onResizeEnd}
            onPointerCancel={onResizeEnd}
          />
        )}
      </div>
    )}
    <div
      ref={shellRef}
      data-overlay={kind}
      className="overlay-shell"
      style={
        {
          transform: `translate(${pos.x}px, ${pos.y}px)`,
          opacity: "var(--overlay-opacity-current, 100%)",
          "--ov-x": base.x,
          "--ov-y": base.y,
          "--ov-w": OVERLAY_DEFAULTS[kind].w,
          "--ov-h": OVERLAY_DEFAULTS[kind].h,
          "--ov-scale": base.scale,
        } as CSSProperties
      }
    >
      {/* Inert while moving: a press anywhere on the sample drags it instead of clicking it. */}
      <div data-overlay-content inert={moving} style={{ display: "contents" }}>
        {children({ tr, ipc, interactive, moving, offset: pos })}
      </div>
    </div>
    </>
  );
}
