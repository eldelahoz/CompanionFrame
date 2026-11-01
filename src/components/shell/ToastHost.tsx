import type { Translator } from "../../lib/i18n";
import { removeToast, useToasts, type ToastLevel } from "../../stores/toasts";
import { Toast } from "../ui";

const TITLE = { info: "toast.info", success: "toast.success", warning: "toast.warning", error: "toast.error" } as const satisfies Record<ToastLevel, string>;

export default function ToastHost({ tr }: { tr: Translator }) {
  const toasts = useToasts();
  return (
    <div
      className="pointer-events-none fixed bottom-[calc(var(--statusbar-height)+var(--space-4))] right-4 z-[1200] flex w-[min(22rem,calc(100vw-2rem))] flex-col gap-2"
      aria-live="polite"
      data-toast-host
    >
      {toasts.map((t) => (
        <Toast key={t.id} tone={t.level} title={t.title || tr(TITLE[t.level])} message={t.message} onDismiss={() => removeToast(t.id)} dismissLabel={tr("common.dismissNotification")} />
      ))}
    </div>
  );
}
