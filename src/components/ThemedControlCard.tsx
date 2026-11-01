import type { ReactNode } from "react";

/** Port of ThemedControlCard.svelte: the bordered row every appearance control sits in. */
export default function ThemedControlCard({ as = "div", density = "default", children }: { as?: "div" | "label"; density?: "default" | "tight"; children: ReactNode }) {
  const base = `border border-[var(--ui-control-border)] rounded-[var(--radius-lg)] bg-[var(--ui-control-bg)] ${density === "tight" ? "py-2 px-2" : "py-2 px-2.5"}`;
  if (as === "label") {
    return <label className={`flex min-w-0 flex-wrap items-center justify-between gap-2.5 cursor-pointer [&>:first-child]:flex-[1_1_5rem] [&>:last-child]:min-w-0 ${base}`}>{children}</label>;
  }
  return <div className={base}>{children}</div>;
}
