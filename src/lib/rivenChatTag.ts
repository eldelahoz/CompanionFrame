import { rivenNameSuffix } from "./marketContract";
import type { DecodedRiven } from "../bindings";

type NamedRiven = Pick<DecodedRiven, "weaponName" | "rivenName">;

/**
 * The in-game link tag, e.g. "[Ack & Brunt Cronitor]". Built from the English
 * game names the chat and warframe.market both key on, never a translated label.
 */
export function rivenChatTag(riven: NamedRiven): string {
  // rivenName already carries the weapon on the decoded side, so the tag is the
  // weapon plus whatever the generated part turns out to be.
  const suffix = rivenNameSuffix(riven.rivenName ?? "", riven.weaponName ?? "");
  const label = `${riven.weaponName ?? ""} ${suffix}`.replace(/\s+/g, " ").trim();
  return label ? `[${label}]` : "";
}

/** The chat line a seller pastes: "WTS [Ack & Brunt Cronitor] 120p". */
export function rivenWtsLine(riven: NamedRiven, platinum: number): string {
  const price = Number.isFinite(platinum) ? Math.max(0, Math.round(platinum)) : 0;
  return `WTS ${rivenChatTag(riven)} ${price}p`;
}
