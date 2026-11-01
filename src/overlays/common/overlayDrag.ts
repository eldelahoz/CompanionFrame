export interface OverlayDragOptions {
  isInteractive: () => boolean;
  moveBy: (dx: number, dy: number) => void;
  /** A drag started (true) or ended (false). */
  onDrag?: (active: boolean) => void;
}

/** Left-drag outside controls or right-drag anywhere, one move per frame. */
export function installOverlayDrag(opts: OverlayDragOptions, doc: Document = document): () => void {
  const root = doc.documentElement;
  let button = -1;
  let last = { x: 0, y: 0 };
  let pending = { dx: 0, dy: 0 };
  let scheduled = false;
  const flush = () => {
    scheduled = false;
    const { dx, dy } = pending;
    pending = { dx: 0, dy: 0 };
    if (dx || dy) opts.moveBy(dx, dy);
  };
  const stop = () => {
    if (button >= 0) opts.onDrag?.(false);
    button = -1;
    pending = { dx: 0, dy: 0 };
    root.classList.remove("is-overlay-dragging");
  };
  const onContext = (e: Event) => {
    if (opts.isInteractive()) e.preventDefault();
  };
  const onDown = (e: MouseEvent) => {
    if (!opts.isInteractive() || (e.button !== 0 && e.button !== 2)) return;
    if (e.button === 0 && e.target instanceof Element && e.target.closest("button, a, input, select, [data-no-drag]")) return;
    e.preventDefault();
    button = e.button;
    last = { x: e.screenX, y: e.screenY };
    root.classList.add("is-overlay-dragging");
    opts.onDrag?.(true);
  };
  const onMove = (e: MouseEvent) => {
    if (button < 0) return;
    if (!opts.isInteractive() || (e.buttons & (button === 0 ? 1 : 2)) === 0) return stop();
    pending.dx += e.screenX - last.x;
    pending.dy += e.screenY - last.y;
    last = { x: e.screenX, y: e.screenY };
    if (!scheduled) {
      scheduled = true;
      requestAnimationFrame(flush);
    }
  };
  const onUp = (e: MouseEvent) => {
    if (e.button === button) stop();
  };
  doc.addEventListener("contextmenu", onContext);
  doc.addEventListener("mousedown", onDown);
  doc.addEventListener("mousemove", onMove);
  doc.addEventListener("mouseup", onUp);
  window.addEventListener("blur", stop);
  return () => {
    doc.removeEventListener("contextmenu", onContext);
    doc.removeEventListener("mousedown", onDown);
    doc.removeEventListener("mousemove", onMove);
    doc.removeEventListener("mouseup", onUp);
    window.removeEventListener("blur", stop);
    stop();
  };
}
