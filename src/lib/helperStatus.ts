import type { MessageKey } from "./i18n";
import type { HelperStatus } from "./ipc";

const INVENTORY_OLD_MS = 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;

type HelperDot = "bg-text-muted" | "bg-warning" | "bg-warning-dim" | "bg-success" | "bg-danger";
export interface HelperView {
  status: { key: MessageKey; params?: Record<string, string> };
  tooltip: MessageKey;
  dot: HelperDot;
  pulse: boolean;
}

/** Clock time, plus the date once the data is over a day old. */
function formatHelperTime(ms: number, now: number, locale: string): string {
  const d = new Date(ms);
  const time = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  return now - ms > DAY_MS ? `${d.toLocaleDateString(locale, { month: "short", day: "numeric" })}, ${time}` : time;
}

/** What the titlebar chip says about the inventory source. */
export function helperView(status: HelperStatus | null, withoutInventory: boolean, linux: boolean, now: number, locale: string): HelperView {
  if (withoutInventory) return { status: { key: "settings.inventorySourceNone" }, tooltip: "setup.source.none.desc", dot: "bg-text-muted", pulse: false };
  const denied = linux ? (["titlebar.status.memoryBlocked", "titlebar.tooltip.memoryBlocked"] as const) : (["titlebar.status.accessDenied", "titlebar.tooltip.accessDenied"] as const);
  const reason = status?.lastRunReason;
  const tooltip: MessageKey = !status?.exeFound
    ? "titlebar.tooltip.helperNotFound"
    : reason === "access-denied"
      ? denied[1]
      : reason === "not-logged-in"
        ? "titlebar.tooltip.notLoggedIn"
        : reason === "token-not-found"
          ? "titlebar.tooltip.tokenNotFound"
          : reason === "game-not-running"
            ? "titlebar.tooltip.gameNotRunning"
            : "titlebar.tooltip.active";
  if (!status) return { status: { key: "titlebar.status.unknown" }, tooltip, dot: "bg-text-muted", pulse: false };
  const modified = status.inventoryLastModified;
  const old = modified != null && now - modified > INVENTORY_OLD_MS;
  const dot: HelperDot = status.running ? "bg-warning" : modified ? (old ? "bg-warning-dim" : "bg-success") : "bg-danger";
  const msg = (key: MessageKey, params?: Record<string, string>) => ({ status: { key, params }, tooltip, dot, pulse: status.running });
  if (status.running) return msg("titlebar.status.refreshing");
  if (modified) return msg(old ? "titlebar.status.old" : "titlebar.status.ok", { time: formatHelperTime(modified, now, locale) });
  if (!status.exeFound) return msg("titlebar.status.helperNotFound");
  if (reason === "access-denied") return msg(denied[0]);
  if (reason === "not-logged-in") return msg("titlebar.status.waitingLogin");
  if (reason === "token-not-found") return msg("titlebar.status.tokenNotFound");
  if (reason === "game-not-running") return msg("titlebar.status.gameNotRunning");
  return msg("titlebar.status.dataMissing");
}

type PillTone = "success" | "warning" | "danger" | "muted";
export interface HelperPill {
  label: MessageKey;
  tone: PillTone;
  pulse: boolean;
}

/** The titlebar pill's short state: connected / syncing / waiting for game / not configured / error. Details stay in helperView. */
export function helperPill(status: HelperStatus | null, withoutInventory: boolean, now: number): HelperPill {
  if (withoutInventory) return { label: "titlebar.pill.notConfigured", tone: "muted", pulse: false };
  if (!status) return { label: "titlebar.pill.checking", tone: "muted", pulse: false };
  if (status.running) return { label: "titlebar.pill.syncing", tone: "warning", pulse: true };
  const modified = status.inventoryLastModified;
  if (modified) return { label: "titlebar.pill.connected", tone: now - modified > INVENTORY_OLD_MS ? "warning" : "success", pulse: false };
  if (!status.exeFound) return { label: "titlebar.pill.notConfigured", tone: "muted", pulse: false };
  const reason = status.lastRunReason;
  if (reason === "game-not-running" || reason === "not-logged-in") return { label: "titlebar.pill.waiting", tone: "warning", pulse: false };
  return { label: "titlebar.pill.error", tone: "danger", pulse: false };
}
