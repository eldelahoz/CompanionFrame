import { useEffect, useState } from "react";
import { helperPill, helperView } from "../../lib/helperStatus";
import type { Translator } from "../../lib/i18n";
import { getHelperStatus, type HelperStatus } from "../../lib/ipc";
import { openView } from "../../stores/nav";

const POLL_MS = 5_000;
const DOT = { success: "bg-success", warning: "bg-warning", danger: "bg-danger", muted: "bg-text-muted" } as const;
const LINUX = typeof navigator !== "undefined" && /linux/i.test(navigator.userAgent);

/** Titlebar pill for warframe-api-helper; App renders it only in game-memory mode. */
export default function HelperStatusPill({ tr, locale }: { tr: Translator; locale: string }) {
  const [helper, setHelper] = useState<HelperStatus | null>(null);
  useEffect(() => {
    let live = true;
    const refresh = () => void getHelperStatus().then((s) => live && setHelper(s), () => {});
    refresh();
    const timer = setInterval(refresh, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);
  const now = Date.now();
  const v = helperView(helper, false, LINUX, now, locale);
  const pill = helperPill(helper, false, now);
  const tip = `${tr(v.status.key, v.status.params)}. ${tr(v.tooltip)} ${tr("titlebar.pill.openSettings")}`;
  return (
    <button type="button" className="titlebar-pill" data-tip={tip} aria-label={tip} data-helper-status={pill.tone} onClick={() => openView("settings")}>
      <span className={`status-dot ${DOT[pill.tone]} ${pill.pulse ? "animate-pulse" : ""}`} />
      <span className="titlebar-pill-text">{tr(pill.label)}</span>
    </button>
  );
}
