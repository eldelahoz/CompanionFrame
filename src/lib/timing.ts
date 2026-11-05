/** Diagnostic timings, dev builds only (there is no frontend-to-log bridge): `timing: <name> k=v ...` in the devtools console. */
export function logTiming(name: string, fields: Record<string, number | string>): void {
  if (import.meta.env.DEV) console.info(`timing: ${name} ${Object.entries(fields).map(([k, v]) => `${k}=${v}`).join(" ")}`);
}

export const msSince = (t0: number): number => Math.round(performance.now() - t0);
