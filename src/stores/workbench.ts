import { useEffect, useState } from "react";
import { onWorkbenchUpdated, workbenchGetState } from "../lib/ipc";
import type { WorkbenchState } from "../../config/shared/tradeWorkbenchTypes";

/** Journal/run state of the sell workbench; null until the first read. Follows `workbench-updated`. */
export function useWorkbenchState(): [WorkbenchState | null, (s: WorkbenchState) => void] {
  const [state, setState] = useState<WorkbenchState | null>(null);
  useEffect(() => {
    let live = true;
    void workbenchGetState().then((s) => live && setState(s));
    const off = onWorkbenchUpdated((s) => live && setState(s));
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, []);
  return [state, setState];
}
