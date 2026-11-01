import { useEffect, useState } from "react";
import ArbiRunsTab from "../components/arbi/ArbiRunsTab";
import { PageHeader, Tabs } from "../components/ui";
import ProfitTakerTab from "../components/profitTaker/ProfitTakerTab";
import type { Translator } from "../lib/i18n";
import { arbiRuns } from "../stores/arbiRuns";
import { ptRuns } from "../stores/ptRuns";
import { usePersisted } from "../lib/usePersisted";

const TABS = ["arbitrations", "profitTaker"] as const;

interface Props {
  tr: Translator;
  language: string;
}

export default function ArbiAnalyzeView({ tr, language }: Props) {
  const [tab, setTab] = usePersisted("arbi-tab", TABS, "arbitrations");
  const [arbiId, setArbiId] = useState<string | null>(null);
  const [ptId, setPtId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Runs saved while this view is open land live; the tabs reload on mount for the ones saved while it was closed.
  useEffect(() => {
    const offs = [arbiRuns.subscribeSaved(), ptRuns.subscribeSaved()];
    return () => void offs.forEach((off) => off.then((f) => f()));
  }, []);

  const refresh = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await Promise.all([arbiRuns.load(), ptRuns.load()]);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto flex w-full max-w-[1280px] flex-col gap-4">
      <PageHeader title={tr("nav.runAnalysis")} />
      <Tabs
        label={tr("nav.runAnalysis")}
        items={[
          { key: "arbitrations", label: tr("common.arbitrations") },
          { key: "profitTaker", label: tr("pt.tab") },
        ]}
        active={tab}
        onSelect={setTab}
      />
      {tab === "arbitrations" && <ArbiRunsTab tr={tr} selectedId={arbiId} onSelect={setArbiId} refresh={refresh} busy={busy} />}
      {tab === "profitTaker" && <ProfitTakerTab tr={tr} locale={language} selectedId={ptId} onSelect={setPtId} refresh={refresh} busy={busy} />}
    </div>
  );
}
