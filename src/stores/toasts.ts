import { useSyncExternalStore } from "react";

export type ToastLevel = "info" | "success" | "warning" | "error";

interface ToastInput {
  level?: ToastLevel;
  title?: string;
  message: string;
  durationMs?: number;
  sticky?: boolean;
}

export interface Toast {
  id: number;
  level: ToastLevel;
  title: string;
  message: string;
  durationMs: number;
  sticky: boolean;
}

const DEFAULT_DURATION_MS = 6000;
const MIN_DURATION_MS = 500;
const MAX_TOASTS = 6;

let nextId = 1;
let toasts: Toast[] = [];
const timers = new Map<number, ReturnType<typeof setTimeout>>();
const listeners = new Set<() => void>();
const publish = (next: Toast[]): void => {
  toasts = next;
  listeners.forEach((l) => l());
};

/** Newest first, capped. */
export function addToast(input: ToastInput): number {
  const id = nextId++;
  const toast: Toast = {
    id,
    level: input.level ?? "info",
    title: input.title ?? "",
    message: input.message,
    durationMs: Math.max(MIN_DURATION_MS, input.durationMs ?? DEFAULT_DURATION_MS),
    sticky: Boolean(input.sticky),
  };
  publish([toast, ...toasts].slice(0, MAX_TOASTS));
  if (!toast.sticky) timers.set(id, setTimeout(() => removeToast(id), toast.durationMs));
  return id;
}

export function removeToast(id: number): void {
  clearTimeout(timers.get(id));
  timers.delete(id);
  publish(toasts.filter((t) => t.id !== id));
}

export const useToasts = (): Toast[] =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => void listeners.delete(l);
    },
    () => toasts,
  );

/** Tests only. */
export function clearToasts(): void {
  timers.forEach(clearTimeout);
  timers.clear();
  publish([]);
}
