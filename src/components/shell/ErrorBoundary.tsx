import { Component, useEffect, type ErrorInfo, type ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import { addToast } from "../../stores/toasts";

const messageOf = (e: unknown, fallback: string): string => (e instanceof Error ? e.message : typeof e === "string" ? e : "") || fallback;

// A sticky toast for a benign observer frame skip is noise on every view.
const isResizeObserverLoop = (e: unknown): boolean => /ResizeObserver loop/i.test(messageOf(e, ""));

/** React boundaries catch render failures only; async errors become toasts without replacing the UI. */
function AsyncErrorToasts({ tr }: { tr: Translator }) {
  useEffect(() => {
    const report = (reason: unknown) =>
      addToast({ level: "error", title: tr("common.rendererErrorTitle"), message: messageOf(reason, tr("common.unknownRendererError")), sticky: true });
    const onError = (e: ErrorEvent) => {
      const reason = e.error ?? e.message;
      if (!isResizeObserverLoop(reason)) report(reason);
    };
    const onRejection = (e: PromiseRejectionEvent) => report(e.reason);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, [tr]);
  return null;
}

const btn = "cursor-pointer rounded border px-3 py-1.5 text-sm transition-[border-color,background] duration-150";
const OK = { error: null as unknown, failed: false };

/** `resetKey` (the active view) clears a crash when the user navigates elsewhere. */
export default class ErrorBoundary extends Component<{ tr: Translator; resetKey?: string; children: ReactNode }, typeof OK> {
  state = OK;

  static getDerivedStateFromError(error: unknown) {
    return { error, failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("[Renderer] render boundary caught", error, info.componentStack);
  }

  componentDidUpdate(prev: { resetKey?: string }) {
    if (this.state.failed && prev.resetKey !== this.props.resetKey) this.setState(OK);
  }

  render() {
    const { tr, children } = this.props;
    return (
      <>
        <AsyncErrorToasts tr={tr} />
        {this.state.failed ? (
          <section className="m-6 rounded-xl border border-danger-dim bg-danger-bg p-5 text-text-primary" role="alert">
            <h2 className="font-display text-2xl tracking-wide">{tr("common.rendererCrashed")}</h2>
            <p className="mt-2 text-sm leading-relaxed text-text-secondary">{messageOf(this.state.error, tr("common.unknownRendererError"))}</p>
            <div className="mt-4 flex flex-wrap gap-2">
              <button className={`${btn} border-danger-dim bg-danger-bg hover:border-danger hover:bg-danger/30`} onClick={() => this.setState(OK)}>
                {tr("common.tryRecover")}
              </button>
              <button className={`${btn} border-border-subtle bg-surface-hover hover:border-border-strong hover:bg-bg-hover`} onClick={() => window.location.reload()}>
                {tr("common.reloadApp")}
              </button>
            </div>
          </section>
        ) : (
          children
        )}
      </>
    );
  }
}
