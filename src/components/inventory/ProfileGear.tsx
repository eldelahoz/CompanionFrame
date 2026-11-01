import { useEffect, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { getPersonalProfile, onProfileInventoryUpdated, type ItemDatabase, type PersonalProfile } from "../../lib/ipc";
import { Badge, Card, CardHeader } from "../ui";
import ItemImage from "../ItemImage";

/**
 * The gear the public profile shows as equipped, for when no inventory is loaded. The profile has
 * no resources, blueprints or counts, so nothing else is derived from it here.
 */
export default function ProfileGear({ tr, items }: { tr: Translator; items: ItemDatabase }) {
  const [profile, setProfile] = useState<PersonalProfile | null>(null);
  useEffect(() => {
    let live = true;
    const load = () => void getPersonalProfile().then((r) => live && setProfile(r.profile ?? null), () => {});
    load();
    const off = onProfileInventoryUpdated(load);
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, []);

  const gear = [...new Map((profile?.appearance ?? []).map((g) => [g.type, g])).values()];
  if (gear.length === 0) return null;
  return (
    <Card data-profile-gear className="mt-3">
      <CardHeader title={tr("inventory.profileGear")} />
      <div className="flex flex-col gap-3 px-4 pb-4">
        <p className="m-0">
          <Badge tone="info">{tr("profile.sourceBadge")}</Badge>
        </p>
        <ul className="ui-list">
          {gear.map((g) => {
            const entry = items[g.type];
            const name = entry?.displayName || entry?.name || g.type.split("/").pop();
            return (
              <li key={g.type} className="text-sm">
                <span className="ui-thumb">
                  <ItemImage src={entry?.imageUrl ?? null} alt="" cls="h-full w-full object-contain" tr={tr} />
                </span>
                <span className="min-w-0 flex-1 truncate">{name}</span>
              </li>
            );
          })}
        </ul>
      </div>
    </Card>
  );
}
