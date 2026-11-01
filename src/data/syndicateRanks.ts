// Typed shim over the rank table (copied from WFHelper, built from DE PublicExport ExportSyndicates via warframe-public-export-plus).
import data from "./syndicateRanks.json";
import type { SyndicateMeta } from "../../config/shared/syndicateTypes";

export const SYNDICATE_RANKS = data.syndicates as SyndicateMeta[];
