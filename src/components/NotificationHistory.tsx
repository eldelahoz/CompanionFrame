import type { Translator } from "../lib/i18n";
import { clearNotificationHistory, removeNotificationEntry, useNotificationHistory } from "../stores/notifications";

const DOT: Record<string, string> = { trade: "bg-success", message: "bg-info", world: "bg-accent" };

/** Newest-first list of everything the notifier delivered. Times are formatted at render so a language switch repaints them. */
export default function NotificationHistory({ tr, locale, onClose }: { tr: Translator; locale: string; onClose: () => void }) {
  const history = useNotificationHistory();
  const when = (at: string) => {
    const d = new Date(at);
    return Number.isNaN(d.getTime()) ? at : d.toLocaleString(locale);
  };
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        role="dialog"
        aria-label={tr("notifications.title")}
        className="detail-panel max-h-[80vh] overflow-y-auto p-4"
        style={{ width: "min(540px, calc(100vw - 3rem))" }}
        data-notification-history
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-start justify-between gap-2">
          <h3 className="m-0 font-display text-lg text-text-primary">{tr("notifications.title")}</h3>
          <div className="flex shrink-0 items-center gap-2">
            <button className="btn-secondary btn-sm" data-notification-clear disabled={history.length === 0} onClick={() => void clearNotificationHistory()}>
              {tr("notifications.clearAll")}
            </button>
            <button className="btn-secondary btn-sm !px-2" aria-label={tr("common.close")} title={tr("common.close")} onClick={onClose}>
              &times;
            </button>
          </div>
        </div>
        {history.length === 0 ? (
          <p className="m-0 py-6 text-center text-sm text-text-muted" data-notification-empty>
            {tr("notifications.empty")}
          </p>
        ) : (
          <ul className="m-0 grid list-none gap-1 p-0">
            {history.map((e) => (
              <li key={e.id} className="grid grid-cols-[auto_1fr_auto] items-start gap-x-2 rounded-md border border-border px-2.5 py-2" data-notification-entry data-notification-kind={e.kind}>
                <span className={`mt-1.5 h-2 w-2 rounded-full ${DOT[e.kind] ?? "bg-text-muted"}`} aria-hidden="true" />
                <div className="min-w-0">
                  <div className="flex items-baseline justify-between gap-2">
                    <span className="truncate text-sm text-text-primary">{e.title}</span>
                    <span className="shrink-0 text-[11px] text-text-muted">{when(e.at)}</span>
                  </div>
                  <p className="m-0 break-words text-xs text-text-muted">{e.body}</p>
                </div>
                <button className="btn-secondary btn-sm !px-1.5 !py-0.5 text-xs" data-notification-remove aria-label={tr("common.dismiss")} title={tr("common.dismiss")} onClick={() => void removeNotificationEntry(e.id)}>
                  &times;
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
