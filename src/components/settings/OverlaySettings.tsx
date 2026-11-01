import { useState } from "react";
import type { Translator } from "../../lib/i18n";
import { hotkeyCapture, overlayLayoutReset, overlayMoveMode, type Settings } from "../../lib/ipc";
import { OVERLAY_OPACITY_MAX_PERCENT, OVERLAY_OPACITY_MIN_PERCENT } from "../../../config/shared/overlayOpacity";
import { OVERLAY_KINDS, type OverlayKind } from "../../overlays/common/overlayTheme";
import CollapsibleSection from "../CollapsibleSection";
import OverlayGuide from "../setup/OverlayGuide";
import OcrLanguageNotice from "./OcrLanguageNotice";

type HotkeyField = "overlay_move_hotkey" | "reward_scan_hotkey" | "riven_rescan_hotkey";
const HOTKEYS: ReadonlyArray<[HotkeyField, "settings.moveOverlaysHotkey" | "settings.rewardScanHotkey" | "settings.rivenRescanHotkey"]> = [
  ["overlay_move_hotkey", "settings.moveOverlaysHotkey"],
  ["reward_scan_hotkey", "settings.rewardScanHotkey"],
  ["riven_rescan_hotkey", "settings.rivenRescanHotkey"],
];
const KIND_LABEL = {
  reward: "settings.overlayKindReward",
  riven: "settings.overlayKindRiven",
  arbi: "settings.overlayKindArbi",
  trade: "settings.overlayKindTrade",
} as const;

/** The relic planner keeps its own offset and scale inside the reward overlay. */
const resetPosition = (kind: string) => {
  for (const key of [`offset:${kind}`, ...(kind === "reward" ? ["offset:planner", "scale:planner"] : [])]) void overlayLayoutReset(key);
};
const button = "rounded border border-border bg-bg-raised px-2 py-1 text-text-primary hover:border-border-strong disabled:opacity-50";

interface Props {
  settings: Settings;
  /** Saves the patch; resolves to the backend's refusal (duplicate hotkey, bad value) or null. */
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
  tr: Translator;
}

export default function OverlaySettings({ settings, onChange, tr }: Props) {
  const [capturing, setCapturing] = useState<HotkeyField | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);

  const save = async (patch: Partial<Settings>) => setError((await onChange(patch)) ?? null);

  const capture = async (field: HotkeyField) => {
    setCapturing(field);
    setError(null);
    try {
      await save({ [field]: await hotkeyCapture() });
    } catch {
      // Esc or timeout: keep the current binding.
    } finally {
      setCapturing(null);
    }
  };

  const enabled = (k: OverlayKind) => settings.overlay_enabled[k] ?? true;
  const opacity = (k: OverlayKind) => settings.overlay_opacity[k] ?? OVERLAY_OPACITY_MAX_PERCENT;

  return (
    <section className="flex flex-col gap-3" aria-label={tr("nav.overlay")}>
      <h3 className="text-text-secondary">{tr("settings.overlayHotkeys")}</h3>
      {HOTKEYS.map(([field, label]) => (
        <div key={field} className="flex items-center justify-between gap-4">
          <span>{tr(label)}</span>
          <button type="button" className={button} disabled={capturing !== null} onClick={() => void capture(field)}>
            {capturing === field ? tr("settings.overlayHotkeyPress") : `${settings[field]}, ${tr("settings.overlayHotkeyChange")}`}
          </button>
        </div>
      ))}
      <p className="text-xs text-text-secondary">{tr("settings.hotkeysHint")}</p>
      {error && (
        <p role="alert" className="text-sm text-red-400">
          {error}
        </p>
      )}
      <h3 className="text-text-secondary">{tr("nav.overlay")}</h3>
      <div className="flex flex-wrap items-center gap-3">
        <button type="button" className={button} onClick={() => void overlayMoveMode(true)}>
          {tr("settings.moveOverlays")}
        </button>
        <span className="text-xs text-text-secondary">{tr("settings.moveOverlaysHint", { hotkey: settings.overlay_move_hotkey || "-" })}</span>
      </div>
      {OVERLAY_KINDS.map((k) => (
        <div key={k} className="flex flex-wrap items-center gap-3">
          <OverlayGuide kind={k} tr={tr} className="w-56 max-w-full" />
          <label className="flex flex-1 items-center gap-2">
            <input type="checkbox" checked={enabled(k)} onChange={(e) => void save({ overlay_enabled: { ...settings.overlay_enabled, [k]: e.target.checked } })} />
            {tr(KIND_LABEL[k])}
          </label>
          <label className="flex items-center gap-2 text-sm text-text-secondary">
            {tr("settings.overlayOpacity")}
            <input
              type="range"
              min={OVERLAY_OPACITY_MIN_PERCENT}
              max={OVERLAY_OPACITY_MAX_PERCENT}
              value={opacity(k)}
              onChange={(e) => void save({ overlay_opacity: { ...settings.overlay_opacity, [k]: Number(e.target.value) } })}
            />
            <span className="w-10 text-right">{opacity(k)}%</span>
          </label>
          <button type="button" className={button} onClick={() => resetPosition(k)}>
            {tr("settings.overlayResetPosition")}
          </button>
        </div>
      ))}
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={settings.message_notifications_enabled} onChange={(e) => void save({ message_notifications_enabled: e.target.checked })} />
        {tr("settings.inGameMessageNotifications")}
      </label>
      <label className={`flex flex-col ${settings.message_notifications_enabled ? "" : "opacity-50"}`}>
        <span className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={settings.message_notifications_while_focused}
            disabled={!settings.message_notifications_enabled}
            onChange={(e) => void save({ message_notifications_while_focused: e.target.checked })}
          />
          {tr("settings.notifyWhileFocused")}
        </span>
        <span className="ml-6 text-sm text-text-secondary">{tr("settings.notifyWhileFocusedHint")}</span>
      </label>
      <CollapsibleSection title={tr("settings.advanced")} collapsed={!advanced} onToggle={() => setAdvanced((a) => !a)}>
        <label className="flex flex-col">
          <span className="flex items-center gap-2">
            <input type="checkbox" checked={settings.save_scan_captures ?? false} onChange={(e) => void save({ save_scan_captures: e.target.checked })} />
            {tr("settings.saveScanCaptures")}
          </span>
          <span className="ml-6 text-sm text-text-secondary">{tr("settings.saveScanCapturesHint")}</span>
        </label>
        <OcrLanguageNotice tr={tr} />
      </CollapsibleSection>
    </section>
  );
}
