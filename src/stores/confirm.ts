import { useSyncExternalStore } from "react";

interface Pending {
  message: string;
  resolve: (ok: boolean) => void;
}

let pending: Pending | null = null;
const listeners = new Set<() => void>();
const publish = (next: Pending | null) => {
  pending = next;
  listeners.forEach((l) => l());
};

/** In-app replacement for window.confirm (Tauri routes that to the dialog plugin, which the app does not grant). Needs <ConfirmHost/> mounted. */
export function askConfirm(message: string): Promise<boolean> {
  pending?.resolve(false);
  return new Promise((resolve) => publish({ message, resolve }));
}

export function answerConfirm(ok: boolean): void {
  pending?.resolve(ok);
  publish(null);
}

export const usePendingConfirm = (): Pending | null =>
  useSyncExternalStore(
    (l) => (listeners.add(l), () => void listeners.delete(l)),
    () => pending,
  );
