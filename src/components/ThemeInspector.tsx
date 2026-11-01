import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { Translator } from "../lib/i18n";
import { buildTokenValueMap, INSPECTED_PROPERTIES, matchComputedColors, toHexInputValue, type InspectedProperty, type TokenMatch } from "../lib/theme/inspector";
import { setThemeInspectorActive, themeSettings, useThemeInspectorActive, useThemeSettings } from "../lib/theme/themeStore";
import { isBaseColorKey } from "../lib/theme/viewOverrides";
import { VIEWS } from "../lib/viewRegistry";
import type { ThemeColors } from "../types/theme";
import type { ViewName } from "../types/views";

interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}
interface UndoEntry {
  colorKey: keyof ThemeColors;
  previous: string;
}

const readRect = (el: Element): Rect => {
  const b = el.getBoundingClientRect();
  return { top: b.top, left: b.left, width: b.width, height: b.height };
};

/** The element under the cursor, ignoring the inspector's own chrome. */
function pickAt(x: number, y: number): Element | null {
  const el = document.elementFromPoint(x, y);
  return !el || el.closest("[data-theme-inspector]") ? null : el;
}

const SWATCH = "h-4 w-4 shrink-0 rounded-[2px] border border-border-subtle";

/** Port of ThemeInspector.svelte: click an element, see which theme tokens paint it, edit them live. */
export default function ThemeInspector({ tr }: { tr: Translator }) {
  const active = useThemeInspectorActive();
  const theme = useThemeSettings();
  const [hoverRect, setHoverRect] = useState<Rect | null>(null);
  const [targetRect, setTargetRect] = useState<Rect | null>(null);
  const [matches, setMatches] = useState<TokenMatch[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [scopedView, setScopedView] = useState<ViewName | null>(null);
  const [undoStack, setUndoStack] = useState<UndoEntry[]>([]);
  const popover = useRef<HTMLDivElement>(null);
  // The window listeners are bound once, so they read the live values through this ref.
  const live = useRef({ active, targetRect });
  live.current = { active, targetRect };

  const selected = matches[selectedIndex] ?? null;
  const liveValue = selected ? theme.colors[selected.colorKey] : "";
  const hexValue = liveValue ? toHexInputValue(liveValue) : "#888888";
  const canScopeToView = selected !== null && scopedView !== null && scopedView !== "settings";
  // Only the hand-picked colours can be scoped; the rest follow from them.
  const scopedKey = selected && isBaseColorKey(selected.colorKey) ? selected.colorKey : null;
  const scopedValue = scopedView && scopedKey ? (theme.viewOverrides[scopedView]?.colors?.[scopedKey] ?? "") : "";

  const close = () => {
    setTargetRect(null);
    setMatches([]);
    setHoverRect(null);
  };
  const exitMode = () => {
    close();
    setThemeInspectorActive(false);
  };

  useEffect(() => {
    if (!active) close();
  }, [active]);

  useEffect(() => {
    if (targetRect) popover.current?.querySelector<HTMLElement>("input, button")?.focus();
  }, [targetRect === null]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    function inspect(el: Element) {
      // The nearest ancestor with data-view scopes a per-view accent, so tokens resolve in context.
      const scope = el.closest("[data-view]");
      const hostStyle = getComputedStyle(scope ?? document.documentElement);
      const tokenMap = buildTokenValueMap((cssVar) => hostStyle.getPropertyValue(cssVar));
      const computed = getComputedStyle(el);
      const values: Partial<Record<InspectedProperty, string>> = {};
      for (const property of INSPECTED_PROPERTIES) {
        const value = computed.getPropertyValue(property);
        if (value) values[property] = value;
      }
      setMatches(matchComputedColors(values, tokenMap));
      setSelectedIndex(0);
      setScopedView((scope?.getAttribute("data-view") as ViewName | null) ?? null);
      setTargetRect(readRect(el));
      setHoverRect(null);
    }
    const onPointerMove = (e: PointerEvent) => {
      if (!live.current.active || live.current.targetRect) return;
      const el = pickAt(e.clientX, e.clientY);
      setHoverRect(el ? readRect(el) : null);
    };
    const onClick = (e: MouseEvent) => {
      const target = e.target;
      if (target instanceof Element && target.closest("[data-theme-inspector]")) return;
      const shortcut = e.ctrlKey && e.shiftKey;
      if (!live.current.active && !shortcut) return;
      e.preventDefault();
      e.stopPropagation();
      const el = pickAt(e.clientX, e.clientY) ?? (target instanceof Element ? target : null);
      if (!el) return;
      if (shortcut) setThemeInspectorActive(true);
      inspect(el);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || (!live.current.active && !live.current.targetRect)) return;
      e.preventDefault();
      e.stopPropagation();
      if (live.current.targetRect) {
        setTargetRect(null);
        setMatches([]);
        setHoverRect(null);
      } else {
        setTargetRect(null);
        setMatches([]);
        setHoverRect(null);
        setThemeInspectorActive(false);
      }
    };
    window.addEventListener("click", onClick, true);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("pointermove", onPointerMove, true);
    return () => {
      window.removeEventListener("click", onClick, true);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("pointermove", onPointerMove, true);
    };
  }, []);

  const pushUndo = (colorKey: keyof ThemeColors) => setUndoStack((s) => [...s, { colorKey, previous: theme.colors[colorKey] }].slice(-30));
  const applyColor = (value: string) => {
    if (!selected) return;
    pushUndo(selected.colorKey);
    themeSettings.setColor(selected.colorKey, value);
  };
  const undo = () => {
    const last = undoStack[undoStack.length - 1];
    if (!last) return;
    setUndoStack(undoStack.slice(0, -1));
    themeSettings.setColor(last.colorKey, last.previous);
  };

  const trapFocus = (e: ReactKeyboardEvent) => {
    if (e.key !== "Tab" || !popover.current) return;
    const focusable = popover.current.querySelectorAll<HTMLElement>("button, input, [href], select, textarea, [tabindex]:not([tabindex='-1'])");
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  if (!active && !targetRect) return null;
  const outline = targetRect ?? hoverRect;
  const viewLabel = scopedView ? (VIEWS.find(([name]) => name === scopedView)?.[1] ?? null) : null;

  return (
    <div data-theme-inspector className="pointer-events-none fixed inset-0 z-[2000]">
      {outline && (
        <div className="absolute rounded-[2px] border-2 border-accent bg-accent-glow" style={{ top: outline.top, left: outline.left, width: outline.width, height: outline.height }} />
      )}
      {targetRect && (
        <div
          ref={popover}
          role="dialog"
          aria-label={tr("appearance.inspector")}
          tabIndex={-1}
          className="pointer-events-auto absolute flex w-[19rem] flex-col gap-2 rounded-[var(--radius-lg)] border border-border-strong bg-bg-surface p-3 shadow-[var(--ui-panel-shadow)]"
          style={{ top: Math.min(targetRect.top + targetRect.height + 8, window.innerHeight - 260), left: Math.min(targetRect.left, window.innerWidth - 320) }}
          onKeyDown={trapFocus}
        >
          <div className="flex items-center justify-between gap-2">
            <span className="font-display text-xs font-semibold tracking-[0.04em] text-text-primary">{tr("appearance.inspector")}</span>
            <button className="btn-secondary btn-sm" type="button" onClick={close}>
              {tr("common.close")}
            </button>
          </div>

          {matches.length === 0 ? (
            <p className="m-0 text-xs text-text-muted">{tr("appearance.inspectorNoTokens")}</p>
          ) : (
            <>
              <div className="flex flex-col gap-1">
                {matches.map((m, i) => (
                  <button
                    key={m.property + m.colorKey}
                    type="button"
                    className={`flex cursor-pointer items-center gap-2 rounded-[var(--radius-sm)] border px-2 py-1 text-left text-xs transition-colors duration-150 ${
                      i === selectedIndex ? "border-accent text-text-primary" : "border-border text-text-secondary hover:border-border-strong"
                    }`}
                    onClick={() => setSelectedIndex(i)}
                  >
                    <span className={SWATCH} style={{ background: theme.colors[m.colorKey] }} />
                    <span className="font-mono">{m.cssVar}</span>
                    <span className="ml-auto text-text-muted">{m.property}</span>
                  </button>
                ))}
              </div>

              <div className="flex items-center gap-2">
                <input
                  type="color"
                  aria-label={tr("appearance.inspectorHexLabel")}
                  className="h-8 w-10 cursor-pointer rounded-[var(--radius-sm)] border border-[var(--ui-control-border)] bg-transparent p-0"
                  value={hexValue}
                  onChange={(e) => applyColor(e.target.value)}
                />
                <input
                  key={liveValue}
                  type="text"
                  aria-label={tr("appearance.inspectorHexLabel")}
                  spellCheck={false}
                  className="min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--ui-control-border)] bg-[var(--ui-control-bg)] px-2 py-1 font-mono text-xs text-text-primary"
                  defaultValue={liveValue}
                  onBlur={(e) => /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i.test(e.target.value.trim()) && applyColor(e.target.value.trim())}
                  onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                />
              </div>

              {canScopeToView && scopedView && (
                <>
                  {scopedValue && (
                    <div className="flex items-center gap-2 text-xs text-text-secondary">
                      <span className={SWATCH} style={{ background: scopedValue }} />
                      <span className="font-mono">{scopedValue}</span>
                      <span className="ml-auto">{viewLabel ? tr(viewLabel) : scopedView}</span>
                    </div>
                  )}
                  <div className="flex gap-1.5">
                    <button
                      className="btn-secondary btn-sm flex-1"
                      type="button"
                      disabled={!scopedKey}
                      title={scopedKey ? undefined : tr("appearance.inspectorScopeBaseOnly")}
                      onClick={() => scopedKey && themeSettings.setViewColor(scopedView, scopedKey, hexValue)}
                    >
                      {tr("appearance.inspectorScopeView")}
                    </button>
                    <button className="btn-secondary btn-sm flex-1" type="button" disabled={!scopedValue} onClick={() => scopedKey && themeSettings.clearViewColor(scopedView, scopedKey)}>
                      {tr("appearance.inspectorScopeGlobal")}
                    </button>
                  </div>
                </>
              )}

              <div className="flex gap-1.5">
                <button className="btn-secondary btn-sm flex-1" type="button" disabled={undoStack.length === 0} onClick={undo}>
                  {tr("mastery.planner.undo")}
                </button>
                <button
                  className="btn-secondary btn-sm flex-1"
                  type="button"
                  onClick={() => {
                    if (!selected) return;
                    pushUndo(selected.colorKey);
                    themeSettings.resetColor(selected.colorKey);
                  }}
                >
                  {tr("common.reset")}
                </button>
              </div>
            </>
          )}

          <button className="btn-danger btn-sm" type="button" onClick={exitMode}>
            {tr("appearance.inspectorExit")}
          </button>
        </div>
      )}
    </div>
  );
}
