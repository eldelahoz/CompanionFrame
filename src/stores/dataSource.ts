import { createSignal } from "../lib/signal";
import type { Settings } from "../lib/ipc";

/** Game-memory mode with consent; mirrors the backend's `memory_gate`. App keeps it in sync with settings. */
export const memoryMode = createSignal(false);

export const isMemoryMode = (s: Pick<Settings, "data_source_mode" | "memory_consent_at">): boolean =>
  s.data_source_mode === "memory" && s.memory_consent_at != null;
