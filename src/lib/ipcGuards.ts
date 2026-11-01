/** Narrows IPC results that came back as `{ error: string }` instead of the happy-path payload. */
export const isIpcError = (value: unknown): value is { error: string } =>
  typeof value === "object" && value !== null && typeof (value as { error?: unknown }).error === "string";
