import { Download } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import { dismissUpdate, installUpdate, openReleaseNotes, useUpdater } from "../../stores/updater";
import { Button, Progress } from "../ui";

/** Persistent, non-blocking notice that a newer release was published; bottom-left so it never covers the toasts. */
export default function UpdateBanner({ tr }: { tr: Translator }) {
  const u = useUpdater();
  const working = u.status === "downloading" || u.status === "installing";
  const failed = u.status === "error" && u.version !== null;
  if (!u.version || (!working && !failed && (u.status !== "available" || u.dismissed))) return null;
  return (
    <article
      className="ui-toast ui-toast--info pointer-events-auto fixed bottom-[calc(var(--statusbar-height)+var(--space-4))] left-4 z-[1200] w-[min(24rem,calc(100vw-2rem))]"
      role={failed ? "alert" : "status"}
      data-update-banner
    >
      <span className="ui-toast-icon">
        <Download aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <strong>{tr("update.available", { version: u.version })}</strong>
        {working ? (
          <>
            <p>{tr(u.status === "installing" ? "update.installing" : "update.downloading")}</p>
            {u.progress !== null && <Progress value={u.progress} label={tr("update.downloading")} />}
          </>
        ) : (
          <>
            {failed && <p className="text-danger">{tr("update.failed", { reason: u.error ?? "" })}</p>}
            <div className="mt-2 flex flex-wrap gap-2">
              <Button size="sm" variant="primary" onClick={() => void installUpdate()}>
                {tr("update.now")}
              </Button>
              <Button size="sm" onClick={openReleaseNotes}>
                {tr("update.notes")}
              </Button>
              <Button size="sm" variant="ghost" onClick={dismissUpdate}>
                {tr("update.later")}
              </Button>
            </div>
          </>
        )}
      </div>
    </article>
  );
}
