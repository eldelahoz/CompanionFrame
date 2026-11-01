import { Bell } from "lucide-react";
import type { Translator } from "../../lib/i18n";

interface Props {
  tr: Translator;
  /** Short text of what the app is doing. */
  status?: string;
  version?: string;
  unread: number;
  onOpenNotifications: () => void;
}

// The update pill waits for the updater.
export default function StatusBar({ tr, status = "", version, unread, onOpenNotifications }: Props) {
  return (
    <footer data-status-bar className="statusbar">
      <span className="statusbar-item">
        <span className="truncate">{status}</span>
      </span>
      <span className="statusbar-right">
        <button type="button" className="statusbar-bell" data-notification-open data-tip={tr("notifications.open")} aria-label={tr("notifications.open")} onClick={onOpenNotifications}>
          <Bell aria-hidden="true" />
          {unread > 0 && (
            <span className="statusbar-count" data-notification-badge>
              {unread}
            </span>
          )}
        </button>
        <span title={tr("statusbar.appVersionTitle")}>v{version ?? "?"}</span>
      </span>
    </footer>
  );
}
