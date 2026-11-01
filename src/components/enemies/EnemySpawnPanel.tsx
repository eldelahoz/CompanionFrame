import { useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardBody, CardHeader, Table } from "../ui";
import type { Translator } from "../../lib/i18n";
import { codexFactionLabel } from "../../lib/codexScans";
import { enemySpawnGroups, factionSpawnPlanets, tileSetSpawnPlanets, type EnemyInfo } from "../../lib/enemies/enemyInfo";
import { SPAWN_NODE_PREVIEW, spawnNodesFor } from "../../lib/enemies/spawnNodes";
import { spawnNodesGet, type SpawnNode } from "../../lib/ipc";
import { buildWikiUrl } from "../../lib/wikiUrl";

type Catalog = { status: "loading" | "failed" } | { status: "ready"; nodes: SpawnNode[] };
let cached: SpawnNode[] | null = null;

/** Star-chart nodes are fetched once, and only when an enemy states a place. */
function useSpawnNodes(wanted: boolean): Catalog {
  const [catalog, setCatalog] = useState<Catalog>(cached ? { status: "ready", nodes: cached } : { status: "loading" });
  useEffect(() => {
    if (!wanted || cached) return;
    let live = true;
    spawnNodesGet().then(
      (nodes) => { cached = nodes; live && setCatalog({ status: "ready", nodes }); },
      () => live && setCatalog({ status: "failed" }),
    );
    return () => { live = false; };
  }, [wanted]);
  return catalog;
}

/** Where an enemy spawns: the wiki's planet / tileset / mission lists and the matching star-chart nodes. */
export default function EnemySpawnPanel({ info, tr }: { info: EnemyInfo; tr: Translator }) {
  const tileSetPlanets = useMemo(() => tileSetSpawnPlanets(info), [info]);
  const factionPlanets = useMemo(() => factionSpawnPlanets(info), [info]);
  const groups = enemySpawnGroups(info, tileSetPlanets);
  const catalog = useSpawnNodes(groups.length > 0);
  const nodes = useMemo(() => (catalog.status === "ready" ? spawnNodesFor(info, catalog.nodes) : []), [info, catalog]);
  const [expandedFor, setExpandedFor] = useState<string | null>(null);
  const all = expandedFor === info.key;
  const shown = all ? nodes : nodes.slice(0, SPAWN_NODE_PREVIEW);
  const faction = codexFactionLabel(info.faction);

  return (
    <Card data-wiki-spawn-panel>
      <CardHeader
        title={<a className="hover:underline" href={buildWikiUrl(info.name)} target="_blank" rel="noreferrer noopener" data-wiki-spawn-enemy={info.name}>{info.name}</a>}
        meta={tr("enemy.spawns")}
        tools={faction ? <Badge>{faction}</Badge> : undefined}
      />
      <CardBody className="flex flex-col gap-2">
        {groups.length > 0 ? (
          groups.map((g) => (
            <div key={g.labelKey} className="flex flex-wrap items-baseline gap-x-3 gap-y-1" data-wiki-spawn-group>
              <span className="w-24 shrink-0 text-xs text-text-muted">{tr(g.labelKey)}</span>
              <span className="min-w-0 flex-1 text-sm text-text-secondary">{g.values.join(", ")}</span>
            </div>
          ))
        ) : factionPlanets.length > 0 && faction ? (
          <div className="flex flex-col gap-2" data-enemy-faction-planets>
            <span className="text-xs text-text-muted">{tr("enemy.factionPlanets", { faction })}</span>
            <div className="flex flex-wrap gap-1.5">
              {factionPlanets.map((p) => <Badge key={p}>{p}</Badge>)}
            </div>
          </div>
        ) : (
          <p className="m-0 text-sm text-text-muted">{tr("enemy.noSpawnData")}</p>
        )}
        {groups.length > 0 && catalog.status === "failed" && <p role="status" className="m-0 text-xs text-text-muted">{tr("enemy.nodesUnavailable")}</p>}
        {groups.length > 0 && catalog.status === "loading" && <p className="m-0 text-xs text-text-muted">{tr("common.loading")}</p>}
        {nodes.length > 0 && (
          <>
            <Table mode="stack" label={tr("enemy.spawns")}>
              <thead>
                <tr>
                  <th>{tr("common.planet")}</th>
                  <th>{tr("common.node")}</th>
                  <th>{tr("common.type")}</th>
                  <th className="ui-num">{tr("common.level")}</th>
                  <th>{tr("enemy.tileSet")}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((n) => (
                  <tr key={n.nodeId} data-enemy-node-row={n.nodeId}>
                    <td data-primary>{n.planet}</td>
                    <td data-label={tr("common.node")}>{n.node}</td>
                    <td data-label={tr("common.type")}>{n.darkSector ? tr("enemy.darkSectorMission", { mission: n.mission }) : n.mission}</td>
                    <td className="ui-num" data-label={tr("common.level")}>{n.minLevel}-{n.maxLevel}</td>
                    <td data-label={tr("enemy.tileSet")}>{n.tileSet}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
            {nodes.length > SPAWN_NODE_PREVIEW && (
              <div>
                <Button size="sm" variant="ghost" data-enemy-nodes-toggle onClick={() => setExpandedFor(all ? null : info.key)}>
                  {all ? tr("common.showFewer") : tr("rivens.detail.showAll", { count: nodes.length })}
                </Button>
              </div>
            )}
          </>
        )}
      </CardBody>
    </Card>
  );
}
