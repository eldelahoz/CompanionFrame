// Workbench contract types come from the generated bindings (the Rust side validates and executes).
import type {
  WorkbenchOverrideAck as _Ack,
  WorkbenchPlanRow_Deserialize as _Row,
  WorkbenchPlan_Deserialize as _Plan,
  WorkbenchSafetySnapshot as _Snap,
  WorkbenchState_Serialize as _State,
} from "../../src/bindings";

export const WORKBENCH_MAX_ROWS_PER_RUN = 100;

export type WorkbenchPlanRow = _Row;
export type WorkbenchPlan = _Plan;
export type WorkbenchSafetySnapshot = _Snap;
export type WorkbenchState = _State;
export type WorkbenchOverrideAck = _Ack;
