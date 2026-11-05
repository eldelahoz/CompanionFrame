import { Wrench } from "lucide-react";
import { devTradeSample, type DevTradeSample } from "../../lib/ipc";
import { Button, Card, CardHeader } from "../ui";

const SAMPLES: Array<[DevTradeSample, string]> = [
  ["OneListing", "One listing"],
  ["TwoListings", "Two listings"],
  ["Closed", "Closed + message copied"],
  ["Purchase", "Purchase"],
  ["NoListing", "No listing"],
  ["PlainTrade", "Plain trade"],
];

/** Development builds only (English on purpose: it never ships, so it has no locale keys). */
export default function DeveloperCard() {
  if (!import.meta.env.DEV) return null;
  return (
    <Card className="settings-card ui-cq" data-settings-card="developer">
      <CardHeader
        title={
          <span className="inline-flex items-center gap-2">
            <Wrench />
            Developer
          </span>
        }
      />
      <div className="settings-card-body">
        <p className="m-0 text-xs text-text-muted">Shows the trade popup with sample data. Nothing is sent to warframe.market.</p>
        <div className="flex flex-wrap gap-2">
          {SAMPLES.map(([sample, label]) => (
            <Button key={sample} size="sm" onClick={() => void devTradeSample(sample)}>
              {label}
            </Button>
          ))}
        </div>
      </div>
    </Card>
  );
}
