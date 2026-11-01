// One entry point for every overlay window (replaces WFHelper's preload-*.ts). Outside Tauri it is inert.
import { listen } from "@tauri-apps/api/event";
import { commands, type PlacementEntry } from "../../bindings";
import { getSettings, overlayLayoutGet, overlayLayoutReset, overlayLayoutSave, overlayMoveMode, overlayPlacementGet, overlayPlacementSaveScale, inTauri, type Settings } from "../../lib/ipc";
import type { OverlayKind } from "./overlayTheme";

type Rect = { x: number; y: number; w: number; h: number };

export interface OverlayIpc {
  /** Subscribe to a backend event; returns the unsubscribe. */
  on<T>(event: string, cb: (payload: T) => void): () => void;
  settings(): Promise<Settings>;
  /** Layout JSON kept in the Rust settings store. */
  layout: { get(key: string): Promise<string | null>; save(key: string, json: string): Promise<void>; reset(key: string): Promise<void> };
  /** Where Setup placed the overlays for this monitor size (physical pixels). */
  placement(width: number, height: number): Promise<PlacementEntry[]>;
  /** Saves the panel scale for this monitor size (move mode's corner handle). */
  saveScale?(width: number, height: number, kind: OverlayKind, scale: number): Promise<void>;
  /** Move mode: the clickable area (physical screen px), whole-window clicks while dragging, Done. */
  move?: {
    regions(kind: OverlayKind, rects: Rect[]): Promise<unknown>;
    interactive(kind: OverlayKind, on: boolean): Promise<unknown>;
    done(): Promise<void>;
  };
}


export const tauriOverlayIpc: OverlayIpc = {
  on(event, cb) {
    if (!("__TAURI_INTERNALS__" in window)) return () => {};
    const un = listen<never>(event, (e) => cb(e.payload));
    return () => void un.then((f) => f());
  },
  settings: getSettings,
  layout: { get: overlayLayoutGet, save: overlayLayoutSave, reset: overlayLayoutReset },
  placement: overlayPlacementGet,
  saveScale: overlayPlacementSaveScale,
  move: {
    regions: async (kind, rects) => inTauri() && commands.overlaySetRegions(kind, rects),
    interactive: async (kind, on) => inTauri() && commands.overlaySetInteractive(kind, on),
    done: () => overlayMoveMode(false),
  },
};
